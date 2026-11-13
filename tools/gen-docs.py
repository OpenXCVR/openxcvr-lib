#!/usr/bin/env python3
"""Compatibility entry point for the canonical Markdown codegen emitter.

Usage: python3 tools/gen-docs.py [schemas-root] [out-root]

The positional paths and historical docs/generated default are retained.
Generation delegates to tools/regen-markdown.sh (same IR, template/include
expansion, revision resolver and validation as bindings), not a separate YAML
semantics engine. The default spec set matches bindings: CMIS, SFF-8636 and
SFF-8472, plus imported SFF-8024 enums. tools/regen-markdown.sh
--print-default-specs is the owner of that set.

A schemas root that carries only a subset of the default protocols selects
exactly the protocols present, discovered the way the schema loader discovers
specs (`<root>/<spec>/_index.yaml`); the run then covers that subset, and a
full root stays equivalent to passing that default set explicitly. A root with
none of the default protocols fails, as do missing required inputs (e.g. an
imported sff-8024 tree). Other namespaces in the root (e.g. C-CMIS, ELSFP) are
never opted into merely because their directories exist. Committed reference
docs use tools/regen-markdown.sh with no spec names, which renders every
namespace that has schemas/<name>/_index.yaml.
"""

from __future__ import annotations

import subprocess
import sys
from pathlib import Path


def default_specs(script: Path) -> list[str]:
    """The default spec set, asked from its single owner (regen-markdown.sh)."""
    proc = subprocess.run(
        [str(script), "--print-default-specs"],
        check=False,
        capture_output=True,
        text=True,
    )
    if proc.returncode != 0 or not proc.stdout.split():
        print(
            proc.stderr.strip()
            or f"{script.name} did not report a default spec set",
            file=sys.stderr,
        )
        raise SystemExit(proc.returncode or 1)
    return proc.stdout.split()


def main() -> int:
    if len(sys.argv) > 3:
        print(__doc__, file=sys.stderr)
        return 2
    root = Path(sys.argv[1] if len(sys.argv) > 1 else "schemas").resolve()
    out = Path(sys.argv[2] if len(sys.argv) > 2 else "docs/generated").resolve()
    script = Path(__file__).resolve().with_name("regen-markdown.sh")

    specs = default_specs(script)
    # Select the default protocols this root still carries. Presence is the
    # `<spec>/_index.yaml` the loader itself discovers specs by, so a pruned
    # root selects exactly the protocols it contains — never extra namespaces
    # that merely have directories, and never an empty selection.
    present = [spec for spec in specs if (root / spec / "_index.yaml").is_file()]
    if not present:
        print(
            f"no default spec roots under {root} "
            f"(looked for: {' '.join(specs)})",
            file=sys.stderr,
        )
        return 1

    return subprocess.run(
        [str(script), "--schemas", str(root), "--out", str(out), *present],
        check=False,
    ).returncode


if __name__ == "__main__":
    raise SystemExit(main())
