#!/usr/bin/env python3
"""Corrective pass for the 2026-04-28 SFF-8024 v2 bulk authoring.

Codex authored every entry in `host-ei-ids.yaml`, `mmf-media-ids.yaml`,
`smf-media-ids.yaml`, `copper-cable-passive.yaml`, `copper-cable-active.yaml`,
and `base-t-codes.yaml` with `reach: {kind: <table-default>}` — including
reserved / unspecified / control entries that § 3 says should use
`reach.kind: none` (entries with no actual media).

This script normalizes `reach.kind` per by name:

- `undefined`, `unspecified`, `unknown_*`, `reserved_*`, `end_of_list`,
  `overflow_escape` → `none` (entries with no committed media).
- `vendor_specific` → the file's table-default kind (vendor-defined entries
  inherit the slot's category — a vendor-specific MMF entry is still MMF,
  the *parameters* are vendor-defined, not the *kind*).
- All other names: untouched.

This corrects two prior issues in one pass:

1. Initial codex output assigned the table-default kind to non-interface
   entries; this rewrites them to `none`.
2. The first version of this script over-applied the `none` rule and
   incorrectly rewrote `vendor_specific` entries; this rewrites those back
   to the table default.

Idempotent: running it twice is a no-op.
"""

from __future__ import annotations

import re
import sys
from pathlib import Path

NON_INTERFACE_PREFIXES: tuple[str, ...] = (
    "reserved_",
    "unknown_",
)

NON_INTERFACE_NAMES: frozenset[str] = frozenset(
    {
        "undefined",
        "end_of_list",
        "overflow_escape",
        "unspecified",
    }
)

# Per-file table-default reach kind — the category the entire table covers.
# Vendor-specific entries in each file inherit this kind.
TABLE_DEFAULT_KIND: dict[str, str] = {
    "host-ei-ids.yaml": "c2m",
    "mmf-media-ids.yaml": "mmf",
    "smf-media-ids.yaml": "smf",
    "copper-cable-passive.yaml": "passive_copper",
    "copper-cable-active.yaml": "active_copper",
    "base-t-codes.yaml": "base_t",
}

FILES: list[Path] = [Path("schemas/sff-8024") / name for name in TABLE_DEFAULT_KIND]

ENTRY_BOUNDARY = re.compile(r"^  - value:")
NAME_LINE = re.compile(r"^    name:\s*(\S+)\s*$")
REACH_LINE = re.compile(r"^(    reach:\s*\{kind:\s*)([a-z0-9_]+)(\}\s*)$")


def is_non_interface(name: str) -> bool:
    if name in NON_INTERFACE_NAMES:
        return True
    return any(name.startswith(prefix) for prefix in NON_INTERFACE_PREFIXES)


def target_kind_for(name: str, table_default: str) -> str | None:
    """Return the kind this entry's `reach` should have, or None to leave alone."""
    if name == "vendor_specific":
        return table_default
    if is_non_interface(name):
        return "none"
    return None


def fix_file(path: Path) -> tuple[int, int]:
    """Returns (rewrites_to_none, rewrites_to_table_default)."""
    lines = path.read_text(encoding="utf-8").splitlines(keepends=True)
    table_default = TABLE_DEFAULT_KIND[path.name]

    entry_starts: list[int] = [
        i for i, line in enumerate(lines) if ENTRY_BOUNDARY.match(line)
    ]
    entry_ends = entry_starts[1:] + [len(lines)]

    none_fixes = 0
    default_fixes = 0
    for start, end in zip(entry_starts, entry_ends):
        name: str | None = None
        for i in range(start, end):
            m = NAME_LINE.match(lines[i])
            if m:
                name = m.group(1)
                break
        if name is None:
            continue
        target = target_kind_for(name, table_default)
        if target is None:
            continue

        for i in range(start, end):
            m = REACH_LINE.match(lines[i])
            if m and m.group(2) != target:
                lines[i] = f"{m.group(1)}{target}{m.group(3)}"
                if target == "none":
                    none_fixes += 1
                else:
                    default_fixes += 1

    if none_fixes or default_fixes:
        path.write_text("".join(lines), encoding="utf-8")
    return none_fixes, default_fixes


def main() -> int:
    total_none = 0
    total_default = 0
    for path in FILES:
        if not path.exists():
            print(f"SKIP {path} — does not exist", file=sys.stderr)
            continue
        none_fixes, default_fixes = fix_file(path)
        total_none += none_fixes
        total_default += default_fixes
        if none_fixes or default_fixes:
            print(
                f"{path}: {none_fixes} → none, "
                f"{default_fixes} → {TABLE_DEFAULT_KIND[path.name]}"
            )
        else:
            print(f"{path}: no changes")
    print(
        f"Total: {total_none} → none, "
        f"{total_default} → table-default kind"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
