#!/usr/bin/env python3
"""Gate Regeneration Equality checker.

Verifies that running tools/regen-all.sh produces zero drift compared to the
committed repository state. Detects:
  - Tracked modifications (files edited by the generator)
  - Untracked generated additions (new files produced by the generator)
  - Removals (files deleted by the generator)

By default, enforces a clean git working copy before regeneration across all
environments (local and CI), proving committed repository equality.

When --allow-dirty is explicitly specified, verifies local generator idempotence
only (i.e. running regeneration reproduces the pre-existing working tree state).
This mode permits dirty schemas matching dirty outputs or unrelated working tree
changes, but explicitly does NOT prove equality to committed repository state.

Pre- and post-regeneration snapshots propagate read, stat, and unexpected entry
errors fail-closed so incomplete or unreadable inventories cannot falsely pass.
Exact filenames (including Unicode, whitespace, and newlines) are preserved
via NUL-terminated git inventories without line-splitting or stripping.
"""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import shlex
import stat
import subprocess
import sys
from typing import Dict, List, Tuple


def get_git_files(repo_root: Path) -> Tuple[List[str], List[str]]:
    """Return (tracked_files, untracked_files) relative to repo_root.

    Uses NUL-separated listings (git ls-files -z) to preserve exact filenames
    including Unicode, whitespace, and newlines without strip or splitlines.
    Ignores gitignored files (such as target/, __pycache__, etc.).
    Propagates subprocess failures fail-closed.
    """
    tracked_raw = subprocess.check_output(
        ["git", "-C", str(repo_root), "ls-files", "-z"],
    )
    tracked = [os.fsdecode(entry) for entry in tracked_raw.split(b"\0") if entry]

    untracked_raw = subprocess.check_output(
        ["git", "-C", str(repo_root), "ls-files", "-z", "--others", "--exclude-standard"],
    )
    untracked = [os.fsdecode(entry) for entry in untracked_raw.split(b"\0") if entry]

    return tracked, untracked


def snapshot_workspace(repo_root: Path) -> Dict[str, str]:
    """Compute SHA-256 digests for all tracked and non-ignored untracked files.

    Propagates read, stat, and unexpected entry errors fail-closed. Deleted
    tracked files (absent on disk) are omitted from the snapshot so that
    diff_snapshots correctly detects them as removals.
    """
    tracked, untracked = get_git_files(repo_root)
    all_files = sorted(set(tracked + untracked))
    snapshot: Dict[str, str] = {}

    for rel_path in all_files:
        full_path = repo_root / rel_path
        try:
            st = os.lstat(full_path)
        except FileNotFoundError:
            # File is absent on disk (e.g. tracked file deleted in working copy).
            # Omit from snapshot so diff_snapshots reports it as removed.
            continue

        if stat.S_ISREG(st.st_mode):
            hasher = hashlib.sha256()
            with open(full_path, "rb") as f:
                while chunk := f.read(65536):
                    hasher.update(chunk)
            snapshot[rel_path] = hasher.hexdigest()
        elif stat.S_ISLNK(st.st_mode):
            target = os.readlink(full_path)
            snapshot[rel_path] = f"symlink:{hashlib.sha256(target.encode('utf-8', errors='surrogateescape')).hexdigest()}"
        else:
            raise OSError(f"Unexpected entry type for {rel_path} (mode {oct(st.st_mode)})")

    return snapshot


def get_working_tree_status(repo_root: Path) -> List[str]:
    """Return porcelain status lines for tracked modifications and untracked files."""
    out = subprocess.check_output(
        ["git", "-C", str(repo_root), "status", "--porcelain"],
        text=True,
    )
    return [line.rstrip() for line in out.splitlines() if line.strip()]


def diff_snapshots(
    before: Dict[str, str], after: Dict[str, str]
) -> Tuple[List[str], List[str], List[str]]:
    """Compare two workspace snapshots.

    Returns (modified, added, removed) relative paths.
    """
    modified = [p for p in before if p in after and before[p] != after[p]]
    added = [p for p in after if p not in before]
    removed = [p for p in before if p not in after]
    return sorted(modified), sorted(added), sorted(removed)


def check_equality(
    repo_root: Path,
    regen_cmd: List[str] | None = None,
    require_clean: bool = True,
    verbose: bool = True,
) -> int:
    """Execute equality check: snapshot -> regen -> snapshot -> diff.

    When require_clean is True (default), enforces a clean working copy before
    regeneration to verify equality against committed repository state.
    When require_clean is False (--allow-dirty), verifies local generator
    idempotence only, disclaiming committed state equality.
    """
    if regen_cmd is None:
        regen_cmd = [str(repo_root / "tools" / "regen-all.sh")]

    if require_clean:
        dirty_lines = get_working_tree_status(repo_root)
        if dirty_lines:
            print(
                "Error: Working tree is not clean before regeneration (--require-clean enforced):",
                file=sys.stderr,
            )
            for line in dirty_lines:
                print(f"  {line}", file=sys.stderr)
            return 1

    if verbose:
        print(f"Taking pre-regeneration workspace snapshot at: {repo_root}")

    before = snapshot_workspace(repo_root)
    if not before:
        print("Error: empty baseline file inventory before regeneration", file=sys.stderr)
        return 1

    if verbose:
        print(f"Positive baseline inventory: {len(before)} files inventoried.")
        print(f"Executing regeneration command: {' '.join(regen_cmd)}")

    regen_result = subprocess.run(regen_cmd, cwd=str(repo_root))
    if regen_result.returncode != 0:
        print(
            f"Error: regeneration command failed with exit code {regen_result.returncode}",
            file=sys.stderr,
        )
        return regen_result.returncode

    if verbose:
        print("Taking post-regeneration workspace snapshot...")

    after = snapshot_workspace(repo_root)
    if not after:
        print("Error: empty file inventory after regeneration", file=sys.stderr)
        return 1

    modified, added, removed = diff_snapshots(before, after)

    if not modified and not added and not removed:
        if verbose:
            if require_clean:
                print(
                    f"Regeneration equality check PASSED: 0 modified, 0 added, 0 removed "
                    f"across {len(after)} files (verified against committed state)."
                )
            else:
                print(
                    f"Regeneration idempotence check PASSED: 0 modified, 0 added, 0 removed "
                    f"across {len(after)} files. NOTE: Working tree changes were permitted "
                    f"(--allow-dirty); this verifies local generator idempotence, NOT equality "
                    f"to committed repository state."
                )
        return 0

    print("Error: Regeneration drift detected!", file=sys.stderr)
    if modified:
        print(f"Tracked modifications ({len(modified)} file(s)):", file=sys.stderr)
        for p in modified:
            print(f"  modified: {p}", file=sys.stderr)
            diff_proc = subprocess.run(
                ["git", "-C", str(repo_root), "diff", "--", p],
                capture_output=True,
                text=True,
                encoding="utf-8",
                errors="replace",
            )
            if diff_proc.stdout.strip():
                for diff_line in diff_proc.stdout.splitlines()[:20]:
                    print(f"    | {diff_line}", file=sys.stderr)
                if len(diff_proc.stdout.splitlines()) > 20:
                    print("    | ... (diff truncated)", file=sys.stderr)

    if added:
        print(f"Untracked additions ({len(added)} file(s)):", file=sys.stderr)
        for p in added:
            print(f"  added (untracked): {p}", file=sys.stderr)

    if removed:
        print(f"Removals ({len(removed)} file(s)):", file=sys.stderr)
        for p in removed:
            print(f"  removed: {p}", file=sys.stderr)

    return 1


def main() -> int:
    parser = argparse.ArgumentParser(
        description=(
            "Gate Regeneration Equality: verify zero drift from tools/regen-all.sh. "
            "Clean-by-default enforces equality to committed repository state. "
            "--allow-dirty tests local regeneration idempotence only."
        )
    )
    parser.add_argument(
        "--repo-root",
        type=Path,
        default=None,
        help="Path to repository root (defaults to locating via script location)",
    )
    parser.add_argument(
        "--regen-cmd",
        type=str,
        default=None,
        help="Custom regeneration command (defaults to tools/regen-all.sh)",
    )
    group = parser.add_mutually_exclusive_group()
    group.add_argument(
        "--require-clean",
        action="store_true",
        default=True,
        help="Require clean git working copy before regeneration (default; enforced regardless of CI env)",
    )
    group.add_argument(
        "--allow-dirty",
        action="store_false",
        dest="require_clean",
        help=(
            "Allow pre-existing working tree changes before regeneration. "
            "Verifies local regeneration idempotence only; does not prove equality "
            "to committed repository state."
        ),
    )
    parser.add_argument(
        "-q",
        "--quiet",
        action="store_true",
        help="Suppress informational progress messages",
    )

    args = parser.parse_args()

    if args.repo_root is not None:
        repo_root = args.repo_root.resolve(strict=True)
    else:
        repo_root = Path(__file__).resolve().parent.parent

    regen_cmd: List[str] | None = None
    if args.regen_cmd is not None:
        regen_cmd = shlex.split(args.regen_cmd)

    return check_equality(
        repo_root=repo_root,
        regen_cmd=regen_cmd,
        require_clean=args.require_clean,
        verbose=not args.quiet,
    )


if __name__ == "__main__":
    sys.exit(main())
