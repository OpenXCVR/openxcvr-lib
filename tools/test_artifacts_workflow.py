"""Fixture tests for .github/workflows/artifacts.yml pkg-config staging."""

import json
import os
from pathlib import Path
import shutil
import stat
import subprocess
import tempfile
import unittest
import yaml

REPO_ROOT = Path(__file__).resolve().parent.parent
WORKFLOW_PATH = REPO_ROOT / ".github" / "workflows" / "artifacts.yml"

LEGS = (
    ("linux-x86_64", "linux", "x86_64"),
    ("macos-arm64", "macos", "arm64"),
    ("macos-x86_64-cross", "macos", "x86_64"),
    ("windows-x86_64", "windows", "x86_64"),
)

BACKUP_SUFFIXES = (".bak", ".orig", ".backup", "~")


def _load_stage_step(job_id: str) -> str:
    data = yaml.safe_load(WORKFLOW_PATH.read_text(encoding="utf-8"))
    steps = data["jobs"][job_id]["steps"]
    for step in steps:
        if step.get("name") == "Stage defined package":
            return step["run"]
    raise AssertionError(f"missing 'Stage defined package' step in job {job_id}")


def _write_executable(path: Path, content: str) -> None:
    path.write_text(content, encoding="utf-8")
    path.chmod(path.stat().st_mode | stat.S_IXUSR | stat.S_IXGRP | stat.S_IXOTH)


def _find_backup_names(root: Path) -> list[str]:
    hits = []
    if not root.exists():
        return hits
    for p in root.rglob("*"):
        name = p.name
        if any(name.endswith(sfx) for sfx in BACKUP_SUFFIXES) or ".bak." in name or ".bak-" in name:
            hits.append(str(p.relative_to(root)))
    return sorted(hits)


class ArtifactsWorkflowTests(unittest.TestCase):
    def _setup_fixture(self, tmp: Path, platform: str, *, include_pc: bool = True, legacy_bak: bool = False):
        workdir = tmp / "work"
        pkgroot = workdir / "pkgroot"
        (pkgroot / "include" / "openxcvr").mkdir(parents=True)
        (pkgroot / "include" / "openxcvr" / "openxcvr.h").write_text("/* h */\n", encoding="utf-8")
        pc_dir = pkgroot / "lib" / "pkgconfig"
        pc_dir.mkdir(parents=True)

        if platform == "macos":
            (pkgroot / "lib" / "libopenxcvr_c.dylib").write_bytes(b"dylib")
        elif platform == "windows":
            (pkgroot / "bin").mkdir(parents=True)
            (pkgroot / "bin" / "openxcvr_c.dll").write_bytes(b"dll")
            (pkgroot / "lib" / "openxcvr_c.lib").write_bytes(b"imp")
            (pkgroot / "lib" / "openxcvr_c_static.lib").write_bytes(b"stat")
        else:
            (pkgroot / "lib" / "libopenxcvr_c.so").write_bytes(b"so")

        if include_pc:
            (pc_dir / "openxcvr-c.pc").write_text(
                "prefix=/tmp/ci-prefix/pkgroot\n"
                "exec_prefix=${prefix}\n"
                "libdir=${exec_prefix}/lib\n"
                "includedir=${prefix}/include\n\n"
                "Name: openxcvr-c\n"
                "Libs: -L${libdir} -lopenxcvr_c\n",
                encoding="utf-8",
            )
            (pc_dir / "openxcvr-c-rich.pc").write_text(
                "prefix=/tmp/ci-prefix/pkgroot\n"
                "exec_prefix=${prefix}\n"
                "libdir=${exec_prefix}/lib\n"
                "Name: openxcvr-c-rich\n"
                "Libs: -L${libdir} -lopenxcvr_c_rich\n",
                encoding="utf-8",
            )

        if legacy_bak:
            (pc_dir / "legacy.pc.bak").write_text("legacy-bytes\n", encoding="utf-8")
            (pc_dir / "unrelated.txt").write_text("keep-me\n", encoding="utf-8")

        spy_bin = tmp / "spy-bin"
        spy_bin.mkdir()
        spy_log = tmp / "sed_invocations.jsonl"
        real_sed = shutil.which("sed")
        self.assertIsNotNone(real_sed)

        _write_executable(
            spy_bin / "sed",
            "#!/usr/bin/env python3\n"
            "import json, os, pathlib, subprocess, sys\n"
            f"real_sed = {real_sed!r}\n"
            f"log_path = pathlib.Path({str(spy_log)!r})\n"
            f"dist_root = pathlib.Path({str(workdir / 'dist')!r})\n"
            "if os.environ.get('SPY_SED_FAIL_PARTIAL') == '1':\n"
            "    sys.stdout.write('prefix=partial\\n')\n"
            "    sys.stdout.flush()\n"
            "    sys.exit(1)\n"
            "rc = subprocess.run([real_sed, *sys.argv[1:]]).returncode\n"
            "seen_bak = []\n"
            "if dist_root.exists():\n"
            "    for p in dist_root.rglob('*'):\n"
            "        if p.name.endswith('.bak') or '.bak.' in p.name or '.bak-' in p.name:\n"
            "            seen_bak.append(str(p.relative_to(dist_root)))\n"
            "with log_path.open('a', encoding='utf-8') as f:\n"
            "    f.write(json.dumps({'argv': sys.argv[1:], 'seen_bak': seen_bak}) + '\\n')\n"
            "sys.exit(rc)\n",
        )
        _write_executable(
            spy_bin / "install_name_tool",
            "#!/bin/sh\nexit 0\n",
        )

        step_tmp = tmp / "step-tmpdir"
        step_tmp.mkdir()
        github_env = tmp / "github_env"
        github_env.write_text("", encoding="utf-8")

        env = {
            "PATH": f"{spy_bin}:{os.environ['PATH']}",
            "TMPDIR": str(step_tmp),
            "GITHUB_ENV": str(github_env),
            "OPENXCVR_PACKAGE_PLATFORM": platform,
        }
        return workdir, spy_log, step_tmp, github_env, env

    def test_yaml_parses_and_contains_all_four_legs(self):
        data = yaml.safe_load(WORKFLOW_PATH.read_text(encoding="utf-8"))
        self.assertEqual(data["name"], "artifacts")
        for job_id, _platform, _arch in LEGS:
            script = _load_stage_step(job_id)
            self.assertNotIn("sed -i.bak", script, f"{job_id} still uses sed -i.bak")
            self.assertNotIn(".pc.bak", script, f"{job_id} still references .pc.bak")

    def test_stage_steps_rewrite_pc_without_creating_bak_files(self):
        for job_id, platform, arch in LEGS:
            with self.subTest(job=job_id):
                script = _load_stage_step(job_id)
                with tempfile.TemporaryDirectory() as td:
                    tmp = Path(td)
                    workdir, spy_log, step_tmp, github_env, env = self._setup_fixture(tmp, platform)
                    env["ARCH"] = arch
                    proc = subprocess.run(
                        ["bash", "-c", script],
                        cwd=workdir,
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(proc.returncode, 0, f"{job_id} failed:\n{proc.stderr}")

                    dist_dir = workdir / "dist" / f"openxcvr-c-{platform}-{arch}"
                    pc_dir = dist_dir / "lib" / "pkgconfig"
                    self.assertEqual(_find_backup_names(workdir / "dist"), [])

                    records = [json.loads(line) for line in spy_log.read_text(encoding="utf-8").splitlines()]
                    self.assertTrue(records, f"{job_id} did not invoke sed")
                    for rec in records:
                        self.assertEqual(rec["seen_bak"], [], f"{job_id} created sibling backup during sed: {rec}")
                        self.assertFalse(
                            any(arg.startswith("-i") for arg in rec["argv"]),
                            f"{job_id} passed in-place -i to sed: {rec['argv']}",
                        )

                    self.assertEqual(
                        (pc_dir / "openxcvr-c.pc").read_text(encoding="utf-8"),
                        "prefix=${pcfiledir}/../..\n"
                        "exec_prefix=${prefix}\n"
                        "libdir=${exec_prefix}/lib\n"
                        "includedir=${prefix}/include\n\n"
                        "Name: openxcvr-c\n"
                        "Libs: -L${libdir} -lopenxcvr_c\n",
                    )
                    self.assertEqual(
                        (pc_dir / "openxcvr-c-rich.pc").read_text(encoding="utf-8"),
                        "prefix=${pcfiledir}/../..\n"
                        "exec_prefix=${prefix}\n"
                        "libdir=${exec_prefix}/lib\n"
                        "Name: openxcvr-c-rich\n"
                        "Libs: -L${libdir} -lopenxcvr_c_rich\n",
                    )
                    self.assertIn(f"DIST=openxcvr-c-{platform}-{arch}\n", github_env.read_text(encoding="utf-8"))
                    self.assertEqual(list(step_tmp.iterdir()), [], f"{job_id} leaked temp files in TMPDIR")

    def test_stage_steps_leave_legacy_sibling_and_unrelated_files_untouched(self):
        for job_id, platform, arch in LEGS:
            with self.subTest(job=job_id):
                script = _load_stage_step(job_id)
                with tempfile.TemporaryDirectory() as td:
                    tmp = Path(td)
                    workdir, _spy_log, step_tmp, _github_env, env = self._setup_fixture(
                        tmp, platform, legacy_bak=True
                    )
                    env["ARCH"] = arch
                    proc = subprocess.run(
                        ["bash", "-c", script],
                        cwd=workdir,
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    self.assertEqual(proc.returncode, 0, proc.stderr)
                    pc_dir = workdir / "dist" / f"openxcvr-c-{platform}-{arch}" / "lib" / "pkgconfig"
                    self.assertEqual((pc_dir / "legacy.pc.bak").read_text(encoding="utf-8"), "legacy-bytes\n")
                    self.assertEqual((pc_dir / "unrelated.txt").read_text(encoding="utf-8"), "keep-me\n")
                    self.assertEqual(list(step_tmp.iterdir()), [])

    def test_stage_steps_abort_before_replacing_pc_when_render_fails(self):
        """A render that writes partial output and fails leaves every original .pc byte-identical."""
        for job_id, platform, arch in LEGS:
            with self.subTest(job=job_id):
                script = _load_stage_step(job_id)
                with tempfile.TemporaryDirectory() as td:
                    tmp = Path(td)
                    workdir, _spy_log, step_tmp, github_env, env = self._setup_fixture(tmp, platform)
                    env["ARCH"] = arch
                    env["SPY_SED_FAIL_PARTIAL"] = "1"
                    before = {p.name: p.read_bytes() for p in (workdir / "pkgroot" / "lib" / "pkgconfig").glob("*.pc")}
                    self.assertEqual(len(before), 2)
                    proc = subprocess.run(
                        ["bash", "-c", script],
                        cwd=workdir,
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    self.assertNotEqual(proc.returncode, 0, f"{job_id} hid a failed render")
                    pc_dir = workdir / "dist" / f"openxcvr-c-{platform}-{arch}" / "lib" / "pkgconfig"
                    after = {p.name: p.read_bytes() for p in pc_dir.glob("*.pc")}
                    self.assertEqual(after, before, f"{job_id} changed a .pc after a failed render")
                    self.assertEqual(github_env.read_text(encoding="utf-8"), "")
                    self.assertEqual(list(step_tmp.iterdir()), [], f"{job_id} leaked temp files in TMPDIR")

    def test_stage_steps_fail_loudly_when_pc_files_are_missing_and_clean_tmpdir(self):
        for job_id, platform, arch in LEGS:
            with self.subTest(job=job_id):
                script = _load_stage_step(job_id)
                with tempfile.TemporaryDirectory() as td:
                    tmp = Path(td)
                    workdir, _spy_log, step_tmp, github_env, env = self._setup_fixture(
                        tmp, platform, include_pc=False
                    )
                    env["ARCH"] = arch
                    proc = subprocess.run(
                        ["bash", "-c", script],
                        cwd=workdir,
                        env=env,
                        capture_output=True,
                        text=True,
                    )
                    self.assertNotEqual(proc.returncode, 0)
                    self.assertEqual(github_env.read_text(encoding="utf-8"), "")
                    self.assertEqual(list(step_tmp.iterdir()), [])


if __name__ == "__main__":
    unittest.main()
