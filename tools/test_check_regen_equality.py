"""Unit tests for tools/check-regen-equality.py.

Dependency-free (Python standard library unittest).
Run: python3 -m unittest tools.test_check_regen_equality -v
"""

from __future__ import annotations

import importlib.util
import os
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

_CHECK_SCRIPT = Path(__file__).resolve().parent / "check-regen-equality.py"
_SPEC = importlib.util.spec_from_file_location("check_regen_equality", _CHECK_SCRIPT)
check_regen_equality = importlib.util.module_from_spec(_SPEC)
sys.modules["check_regen_equality"] = check_regen_equality
_SPEC.loader.exec_module(check_regen_equality)


class TestCheckRegenEquality(unittest.TestCase):
    def setUp(self):
        self.temp_dir = tempfile.TemporaryDirectory()
        self.repo_root = Path(self.temp_dir.name)
        # Initialize a minimal git repository
        subprocess.run(["git", "init", "-b", "main"], cwd=str(self.repo_root), check=True, capture_output=True)
        subprocess.run(["git", "config", "user.name", "Test User"], cwd=str(self.repo_root), check=True)
        subprocess.run(["git", "config", "user.email", "test@example.com"], cwd=str(self.repo_root), check=True)
        subprocess.run(["git", "config", "core.quotePath", "true"], cwd=str(self.repo_root), check=True)

        # Create standard file structure
        (self.repo_root / "schemas").mkdir(parents=True)
        (self.repo_root / "schemas" / "test.yaml").write_text("spec: test\n", encoding="utf-8")
        (self.repo_root / "generated").mkdir(parents=True)
        (self.repo_root / "generated" / "out.txt").write_text("initial output\n", encoding="utf-8")

        subprocess.run(["git", "add", "."], cwd=str(self.repo_root), check=True)
        subprocess.run(["git", "commit", "-m", "initial commit"], cwd=str(self.repo_root), check=True, capture_output=True)

    def tearDown(self):
        self.temp_dir.cleanup()

    def test_clean_run_passes(self):
        """When regeneration makes no changes, check_equality returns 0."""
        # Mock regen command that does nothing
        noop_cmd = [sys.executable, "-c", "import sys; sys.exit(0)"]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=noop_cmd,
            require_clean=False,
            verbose=False,
        )
        self.assertEqual(code, 0)

    def test_tracked_modification_detected(self):
        """When regeneration modifies a tracked file, check_equality returns 1."""
        mod_cmd = [
            sys.executable,
            "-c",
            "from pathlib import Path; "
            "Path('generated/out.txt').write_text('mutated output\\n', encoding='utf-8')",
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=mod_cmd,
            require_clean=False,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_untracked_addition_detected(self):
        """When regeneration creates a new untracked file, check_equality returns 1."""
        add_cmd = [
            sys.executable,
            "-c",
            "from pathlib import Path; "
            "Path('generated/new_surface.txt').write_text('new generated content\\n', encoding='utf-8')",
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=add_cmd,
            require_clean=False,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_removal_detected(self):
        """When regeneration removes an existing file, check_equality returns 1."""
        rm_cmd = [
            sys.executable,
            "-c",
            "from pathlib import Path; Path('generated/out.txt').unlink()",
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=rm_cmd,
            require_clean=False,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_unrelated_working_tree_changes_distinguished(self):
        """Pre-existing unrelated working tree changes are not reported as generator drift."""
        # User is working on a schema and has an untracked scratch note before running regen
        (self.repo_root / "schemas" / "test.yaml").write_text("spec: test\n# work in progress\n", encoding="utf-8")
        (self.repo_root / "scratch.txt").write_text("scratch notes\n", encoding="utf-8")

        # Generator runs and does not change generated output
        noop_cmd = [sys.executable, "-c", "import sys; sys.exit(0)"]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=noop_cmd,
            require_clean=False,
            verbose=False,
        )
        self.assertEqual(code, 0)

    def test_require_clean_enforced(self):
        """In CI or when --require-clean is set, pre-existing dirty working tree fails immediately."""
        (self.repo_root / "scratch.txt").write_text("uncommitted file\n", encoding="utf-8")
        noop_cmd = [sys.executable, "-c", "import sys; sys.exit(0)"]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=noop_cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_failing_regen_command_propagates_exit_code(self):
        """Non-zero exit code from regen command is propagated."""
        fail_cmd = [sys.executable, "-c", "import sys; sys.exit(42)"]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=fail_cmd,
            require_clean=False,
            verbose=False,
        )
        self.assertEqual(code, 42)

    def test_clean_by_default_enforces_clean_working_copy(self):
        """Clean working copy is enforced by default even when require_clean is not specified."""
        (self.repo_root / "scratch.txt").write_text("uncommitted file\n", encoding="utf-8")
        noop_cmd = [sys.executable, "-c", "import sys; sys.exit(0)"]
        # Do not pass require_clean; must default to True (clean-by-default)
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=noop_cmd,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_dirty_stable_generated_content_rejected_by_default(self):
        """Dirty generated outputs matching dirty schemas fail committed equality by default."""
        (self.repo_root / "schemas" / "test.yaml").write_text("spec: test v2\n", encoding="utf-8")
        (self.repo_root / "generated" / "out.txt").write_text("v2 output\n", encoding="utf-8")
        # Generator emits the exact same v2 content
        noop_cmd = [sys.executable, "-c", "import sys; sys.exit(0)"]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=noop_cmd,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_dirty_stable_generated_content_passes_with_allow_dirty(self):
        """Dirty outputs matching dirty schemas are valid local idempotence with require_clean=False."""
        (self.repo_root / "schemas" / "test.yaml").write_text("spec: test v2\n", encoding="utf-8")
        (self.repo_root / "generated" / "out.txt").write_text("v2 output\n", encoding="utf-8")
        # Generator emits the exact same v2 content
        noop_cmd = [sys.executable, "-c", "import sys; sys.exit(0)"]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=noop_cmd,
            require_clean=False,
            verbose=False,
        )
        self.assertEqual(code, 0)

    def test_injected_oserror_pre_snapshot_propagates(self):
        """Deterministic injected OSError during pre-snapshot with readable peer files propagates fail-closed."""
        real_open = open

        def faulty_open(file, *args, **kwargs):
            if "out.txt" in str(file):
                raise OSError(13, "Permission denied (injected pre-snapshot)")
            return real_open(file, *args, **kwargs)

        noop_cmd = [sys.executable, "-c", "import sys; sys.exit(0)"]
        import unittest.mock
        with unittest.mock.patch("builtins.open", side_effect=faulty_open):
            with self.assertRaises(OSError):
                check_regen_equality.check_equality(
                    repo_root=self.repo_root,
                    regen_cmd=noop_cmd,
                    require_clean=False,
                    verbose=False,
                )

    def test_injected_oserror_post_snapshot_propagates(self):
        """Deterministic injected OSError during post-snapshot with readable peer files propagates fail-closed."""
        real_open = open
        regen_marker = self.repo_root / ".regen_ran"

        def faulty_open(file, *args, **kwargs):
            # Only fail after regeneration command has run
            if regen_marker.exists() and "out.txt" in str(file):
                raise OSError(5, "I/O error (injected post-snapshot)")
            return real_open(file, *args, **kwargs)

        # Regen command creates marker file
        marker_cmd = [
            sys.executable,
            "-c",
            "from pathlib import Path; Path('.regen_ran').touch()",
        ]
        import unittest.mock
        with unittest.mock.patch("builtins.open", side_effect=faulty_open):
            with self.assertRaises(OSError):
                check_regen_equality.check_equality(
                    repo_root=self.repo_root,
                    regen_cmd=marker_cmd,
                    require_clean=False,
                    verbose=False,
                )

    def test_empty_post_inventory_rejected(self):
        """Wiping workspace files during regeneration fails post-regeneration positive inventory check."""
        # Regen command removes all workspace files (leaving .git intact)
        wipe_cmd = [
            sys.executable,
            "-c",
            "from pathlib import Path; "
            "Path('schemas/test.yaml').unlink(); "
            "Path('generated/out.txt').unlink()",
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=wipe_cmd,
            require_clean=False,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_unexpected_entry_type_raises(self):
        """Non-regular and non-symlink file entries raise OSError fail-closed."""
        # Mock os.lstat to return a socket or FIFO mode for one file
        import stat
        real_lstat = os.lstat
        def mock_lstat(path, *args, **kwargs):
            st = real_lstat(path, *args, **kwargs)
            if "out.txt" in str(path):
                # Return socket mode
                class FakeStat:
                    st_mode = stat.S_IFSOCK | 0o644
                return FakeStat()
            return st

        import unittest.mock
        with unittest.mock.patch("os.lstat", side_effect=mock_lstat):
            with self.assertRaises(OSError):
                check_regen_equality.snapshot_workspace(self.repo_root)

    def test_git_failure_propagates(self):
        """Git failures during file discovery propagate rather than passing silently."""
        noop_cmd = [sys.executable, "-c", "import sys; sys.exit(0)"]
        import unittest.mock
        with unittest.mock.patch(
            "subprocess.check_output",
            side_effect=subprocess.CalledProcessError(128, ["git", "ls-files"]),
        ):
            with self.assertRaises(subprocess.CalledProcessError):
                check_regen_equality.check_equality(
                    repo_root=self.repo_root,
                    regen_cmd=noop_cmd,
                    require_clean=False,
                    verbose=False,
                )

    def test_empty_baseline_inventory_rejected(self):
        """An empty repository with zero files fails positive inventory check."""
        with tempfile.TemporaryDirectory() as empty_dir:
            empty_root = Path(empty_dir)
            subprocess.run(["git", "init", "-b", "main"], cwd=str(empty_root), check=True, capture_output=True)
            noop_cmd = [sys.executable, "-c", "import sys; sys.exit(0)"]
            code = check_regen_equality.check_equality(
                repo_root=empty_root,
                regen_cmd=noop_cmd,
                require_clean=False,
                verbose=False,
            )
            self.assertEqual(code, 1)


    def test_cli_default_clean_enforces_clean(self):
        """CLI main() requires clean by default without needing flags."""
        (self.repo_root / "scratch.txt").write_text("uncommitted file\n", encoding="utf-8")
        import unittest.mock
        argv = [
            "check-regen-equality.py",
            "--repo-root",
            str(self.repo_root),
            "--regen-cmd",
            f"{sys.executable} -c 'pass'",
            "-q",
        ]
        with unittest.mock.patch.object(sys, "argv", argv):
            code = check_regen_equality.main()
            self.assertEqual(code, 1)

    def test_cli_allow_dirty_permits_dirt(self):
        """CLI main() with --allow-dirty succeeds when working tree is dirty but generated files match."""
        (self.repo_root / "scratch.txt").write_text("uncommitted file\n", encoding="utf-8")
        import unittest.mock
        argv = [
            "check-regen-equality.py",
            "--repo-root",
            str(self.repo_root),
            "--regen-cmd",
            f"{sys.executable} -c 'pass'",
            "--allow-dirty",
            "-q",
        ]
        with unittest.mock.patch.object(sys, "argv", argv):
            code = check_regen_equality.main()
            self.assertEqual(code, 0)

    def test_cli_mutually_exclusive_flags(self):
        """Passing both --require-clean and --allow-dirty fails argument parsing."""
        import unittest.mock
        argv = [
            "check-regen-equality.py",
            "--require-clean",
            "--allow-dirty",
        ]
        with unittest.mock.patch.object(sys, "argv", argv):
            with self.assertRaises(SystemExit):
                check_regen_equality.main()

    def _commit_unusual_paths(self) -> tuple[str, str, str]:
        unicode_path = "generated/μ.rs"
        ws_path = "generated/ padded.txt "
        nl_path = "generated/line1\nline2.txt"

        for p_str, content in [
            (unicode_path, "initial unicode\n"),
            (ws_path, "initial spaces\n"),
            (nl_path, "initial newline\n"),
        ]:
            p = self.repo_root / p_str
            p.parent.mkdir(parents=True, exist_ok=True)
            p.write_text(content, encoding="utf-8")

        subprocess.run(["git", "add", "-A"], cwd=str(self.repo_root), check=True)
        subprocess.run(
            ["git", "commit", "-m", "commit unusual paths"],
            cwd=str(self.repo_root),
            check=True,
            capture_output=True,
        )
        return unicode_path, ws_path, nl_path

    def test_untracked_unicode_addition_detected_clean_mode(self):
        """When regeneration creates an untracked Unicode file, check_equality detects it in clean mode."""
        cmd = [
            sys.executable,
            "-c",
            "import sys, pathlib; p = pathlib.Path(sys.argv[1]); p.parent.mkdir(parents=True, exist_ok=True); p.write_text('fn main() {}\\n', encoding='utf-8')",
            "generated/μ.rs",
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_untracked_whitespace_padded_addition_detected_clean_mode(self):
        """When regeneration creates an untracked whitespace-padded file, check_equality detects it in clean mode."""
        cmd = [
            sys.executable,
            "-c",
            "import sys, pathlib; p = pathlib.Path(sys.argv[1]); p.parent.mkdir(parents=True, exist_ok=True); p.write_text('content\\n', encoding='utf-8')",
            "generated/ padded filename.txt ",
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_untracked_newline_bearing_addition_detected_clean_mode(self):
        """When regeneration creates an untracked newline-bearing file, check_equality detects it in clean mode."""
        cmd = [
            sys.executable,
            "-c",
            "import sys, pathlib; p = pathlib.Path(sys.argv[1]); p.parent.mkdir(parents=True, exist_ok=True); p.write_text('content\\n', encoding='utf-8')",
            "generated/line1\nline2.txt",
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_committed_unusual_paths_clean_baseline_passes(self):
        """Clean baseline with committed Unicode, whitespace-padded, and newline files passes."""
        self._commit_unusual_paths()
        noop_cmd = [sys.executable, "-c", "pass"]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=noop_cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 0)

    def test_committed_unicode_path_modification_detected(self):
        """Modifying a previously committed Unicode file is detected in clean mode."""
        unicode_path, _, _ = self._commit_unusual_paths()
        cmd = [
            sys.executable,
            "-c",
            "import sys, pathlib; pathlib.Path(sys.argv[1]).write_text('mutated\\n', encoding='utf-8')",
            unicode_path,
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_committed_unicode_path_removal_detected(self):
        """Removing a previously committed Unicode file is detected in clean mode."""
        unicode_path, _, _ = self._commit_unusual_paths()
        cmd = [
            sys.executable,
            "-c",
            "import sys, pathlib; pathlib.Path(sys.argv[1]).unlink()",
            unicode_path,
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_committed_whitespace_path_modification_detected(self):
        """Modifying a previously committed whitespace-padded file is detected in clean mode."""
        _, ws_path, _ = self._commit_unusual_paths()
        cmd = [
            sys.executable,
            "-c",
            "import sys, pathlib; pathlib.Path(sys.argv[1]).write_text('mutated\\n', encoding='utf-8')",
            ws_path,
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_committed_whitespace_path_removal_detected(self):
        """Removing a previously committed whitespace-padded file is detected in clean mode."""
        _, ws_path, _ = self._commit_unusual_paths()
        cmd = [
            sys.executable,
            "-c",
            "import sys, pathlib; pathlib.Path(sys.argv[1]).unlink()",
            ws_path,
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_committed_newline_path_modification_detected(self):
        """Modifying a previously committed newline-bearing file is detected in clean mode."""
        _, _, nl_path = self._commit_unusual_paths()
        cmd = [
            sys.executable,
            "-c",
            "import sys, pathlib; pathlib.Path(sys.argv[1]).write_text('mutated\\n', encoding='utf-8')",
            nl_path,
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 1)

    def test_committed_newline_path_removal_detected(self):
        """Removing a previously committed newline-bearing file is detected in clean mode."""
        _, _, nl_path = self._commit_unusual_paths()
        cmd = [
            sys.executable,
            "-c",
            "import sys, pathlib; pathlib.Path(sys.argv[1]).unlink()",
            nl_path,
        ]
        code = check_regen_equality.check_equality(
            repo_root=self.repo_root,
            regen_cmd=cmd,
            require_clean=True,
            verbose=False,
        )
        self.assertEqual(code, 1)


class TestOpenApiTypeScriptRegenGate(unittest.TestCase):
    """OpenAPI to TypeScript cmis.ts must be inside regeneration equality.

    Counterexample this closes: add an optional field to the hand-authored
    OpenAPI document and leave generated/openapi/cmis.ts untouched. Before
    tools/regen-typescript.sh ran `npm run gen:types`, regen-all.sh did not
    rewrite that file, so check-regen-equality reported a clean tree while
    the exported types stayed stale.
    """

    _LANE_ANCHOR = (
        '          description: "Tx failure latch (FailureFlagTx<lane>, '
        '§ 8.10.3 Table 8-87, 11h:135 bit lane-1; latched, clear-on-read)."\n'
        "        tx_power_mw:\n"
    )
    _PROBE = (
        '          description: "Tx failure latch (FailureFlagTx<lane>, '
        '§ 8.10.3 Table 8-87, 11h:135 bit lane-1; latched, clear-on-read)."\n'
        "        stale_probe:\n"
        "          type: string\n"
        '          description: "Regen-equality probe. Not a contract field."\n'
        "        tx_power_mw:\n"
    )

    def _init_git(self, root: Path) -> None:
        subprocess.run(
            ["git", "init", "-b", "main"],
            cwd=str(root),
            check=True,
            capture_output=True,
        )
        subprocess.run(
            ["git", "config", "user.name", "Test User"],
            cwd=str(root),
            check=True,
        )
        subprocess.run(
            ["git", "config", "user.email", "test@example.com"],
            cwd=str(root),
            check=True,
        )

    def test_regen_typescript_script_invokes_gen_types(self):
        repo = Path(__file__).resolve().parents[1]
        text = (repo / "tools" / "regen-typescript.sh").read_text(encoding="utf-8")
        self.assertIn("npm run gen:types", text)
        self.assertIn("OPENXCVR_REGEN_TYPESCRIPT_OPENAPI_ONLY", text)
        all_text = (repo / "tools" / "regen-all.sh").read_text(encoding="utf-8")
        self.assertIn('tools/regen-$lang.sh', all_text)
        self.assertIn("typescript", all_text)
        self.assertIn("generated/openapi/cmis.ts", all_text)

    def test_optional_openapi_field_fails_equality_until_cmis_ts_regenerates(self):
        repo = Path(__file__).resolve().parents[1]
        schema_src = (repo / "bindings" / "openapi" / "openxcvr.openapi.yaml").read_text(
            encoding="utf-8"
        )
        committed = (
            repo / "bindings" / "typescript" / "generated" / "openapi" / "cmis.ts"
        ).read_bytes()
        self.assertEqual(schema_src.count(self._LANE_ANCHOR), 1, "CMIS lane anchor drifted")
        mutated = schema_src.replace(self._LANE_ANCHOR, self._PROBE, 1)
        self.assertIn("stale_probe", mutated)
        self.assertNotIn(b"stale_probe", committed)

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            self._init_git(root)
            schema_path = root / "openapi.yaml"
            schema_path.write_text(mutated, encoding="utf-8")
            out = root / "bindings" / "typescript" / "generated" / "openapi" / "cmis.ts"
            out.parent.mkdir(parents=True)
            out.write_bytes(committed)
            subprocess.run(["git", "add", "."], cwd=str(root), check=True)
            subprocess.run(
                ["git", "commit", "-m", "stale cmis.ts"],
                cwd=str(root),
                check=True,
                capture_output=True,
            )

            regen = ["bash", str(repo / "tools" / "regen-typescript.sh")]
            env_keys = {
                "OPENXCVR_REGEN_TYPESCRIPT_OPENAPI_ONLY": "1",
                "OPENXCVR_OPENAPI_SCHEMA": str(schema_path),
                "OPENXCVR_TS_OPENAPI_OUT": str(out),
            }
            saved = {key: os.environ.get(key) for key in env_keys}
            os.environ.update(env_keys)
            try:
                stale = check_regen_equality.check_equality(
                    repo_root=root,
                    regen_cmd=regen,
                    require_clean=True,
                    verbose=False,
                )
                self.assertEqual(stale, 1)
                regenerated = out.read_text(encoding="utf-8")
                self.assertIn("stale_probe", regenerated)

                fresh = check_regen_equality.check_equality(
                    repo_root=root,
                    regen_cmd=regen,
                    require_clean=False,
                    verbose=False,
                )
                self.assertEqual(fresh, 0)
                self.assertEqual(out.read_text(encoding="utf-8"), regenerated)
            finally:
                for key, previous in saved.items():
                    if previous is None:
                        os.environ.pop(key, None)
                    else:
                        os.environ[key] = previous


if __name__ == "__main__":
    unittest.main()
