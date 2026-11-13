"""Self-test for tools/gen-docs.py partial-root selection.

Binds the partial-root behavior to an actual invocation of the generation
pipeline (python3 tools/gen-docs.py → tools/regen-markdown.sh → cargo), not
only to the subprocess arguments: every test runs the real entry point against
a real schemas root and inspects the generated tree. Requires cargo on PATH.
Run:  python3 -m unittest tools.test_gen_docs -v
"""

import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

REPO_ROOT = Path(__file__).resolve().parent.parent
GEN_DOCS = REPO_ROOT / "tools" / "gen-docs.py"
REGEN_MARKDOWN = REPO_ROOT / "tools" / "regen-markdown.sh"

PAGE_YAML = """\
schema: openxcvr.page.v2
kind: page
id: SyntheticPage
title: Synthetic Page
page_address: 01h
size_bytes: 128
fields:
  - { name: flag, offset: 128, type: u8, access: ro, description: "A field" }
"""

ENUM_YAML = """\
schema: openxcvr.enum.v2
kind: enum
id: SyntheticEnum
title: Synthetic Enum
entries:
  - { value: 0, name: zero, description: Zero }
"""


def _memory_map_index(spec_id: str, title: str) -> str:
    return (
        "schema: openxcvr.schema_index.v1\n"
        f"spec:\n  id: {spec_id}\n  title: {title}\n  revision: \"1.0\"\n"
        "kind: memory_map\n"
        "includes:\n"
        "  - page.yaml\n"
        "  - enum.yaml\n"
    )


def _enum_registry_index(spec_id: str, title: str) -> str:
    return (
        "schema: openxcvr.schema_index.v1\n"
        f"spec:\n  id: {spec_id}\n  title: {title}\n  revision: \"1.0\"\n"
        "kind: enum_registry\n"
        "includes:\n"
        "  - enum.yaml\n"
    )


def _write_spec(root: Path, name: str, index: str) -> None:
    spec = root / name
    spec.mkdir(parents=True)
    (spec / "_index.yaml").write_text(index, encoding="utf-8")
    (spec / "enum.yaml").write_text(ENUM_YAML, encoding="utf-8")
    if "memory_map" in index:
        (spec / "page.yaml").write_text(PAGE_YAML, encoding="utf-8")


def _run(argv: list[str]) -> subprocess.CompletedProcess:
    return subprocess.run(
        argv, check=False, capture_output=True, text=True, cwd=str(REPO_ROOT)
    )


def _tree(root: Path) -> dict[str, bytes]:
    return {
        str(p.relative_to(root)): p.read_bytes()
        for p in sorted(root.rglob("*"))
        if p.is_file()
    }


class PartialRootSelection(unittest.TestCase):
    def test_partial_root_generates_only_present_default_specs(self):
        # A root carrying a valid subset of the default protocols (cmis) plus a
        # non-default namespace that merely exists (c-cmis-1.4): the run must
        # cover cmis, and must not opt into c-cmis-1.4 because its directory
        # exists.
        with tempfile.TemporaryDirectory() as d:
            root = Path(d) / "schemas"
            out = Path(d) / "out"
            _write_spec(root, "cmis", _memory_map_index("cmis", "Synthetic CMIS"))
            _write_spec(
                root, "c-cmis-1.4", _enum_registry_index("c-cmis-1.4", "Synthetic C-CMIS")
            )

            proc = _run([sys.executable, str(GEN_DOCS), str(root), str(out)])
            self.assertEqual(proc.returncode, 0, proc.stderr)

            # cmis was generated.
            self.assertTrue((out / "cmis" / "index.md").is_file())
            pages = list((out / "cmis" / "pages").glob("*.md"))
            self.assertEqual(len(pages), 1, pages)
            self.assertIn("SyntheticPage", pages[0].read_text(encoding="utf-8"))
            # The non-default namespace was not opted into.
            self.assertFalse((out / "c-cmis-1.4" / "index.md").exists())
            self.assertFalse((out / "c-cmis-1.4" / "enums").exists())

            # The top index inventories both namespaces accurately.
            top = (out / "index.md").read_text(encoding="utf-8")
            self.assertIn("rendered", top)
            self.assertIn("`c-cmis-1.4`", top)
            self.assertIn("not rendered", top)

    def test_empty_selection_fails(self):
        # A root with no default-protocol spec directories fails with a clear
        # message instead of silently generating nothing (or everything).
        with tempfile.TemporaryDirectory() as d:
            root = Path(d) / "schemas"
            out = Path(d) / "out"
            _write_spec(root, "sff-8024", _enum_registry_index("sff-8024", "SFF-8024"))

            proc = _run([sys.executable, str(GEN_DOCS), str(root), str(out)])
            self.assertNotEqual(proc.returncode, 0)
            self.assertIn("no default spec roots", proc.stderr)
            self.assertFalse(out.exists())

    def test_full_root_output_matches_canonical_default_set(self):
        # Regression guard: on the full schemas root, the legacy entry point
        # must produce byte-identical output to regen-markdown.sh invoked with
        # the binding default specs. No-argument regen-markdown renders every
        # namespace index and is intentionally wider than this entry point.
        schemas = REPO_ROOT / "schemas"
        with tempfile.TemporaryDirectory() as d:
            legacy_out = Path(d) / "legacy"
            canonical_out = Path(d) / "canonical"
            defaults = _run([str(REGEN_MARKDOWN), "--print-default-specs"])
            self.assertEqual(defaults.returncode, 0, defaults.stderr)
            default_specs = defaults.stdout.split()

            legacy = _run([sys.executable, str(GEN_DOCS), str(schemas), str(legacy_out)])
            self.assertEqual(legacy.returncode, 0, legacy.stderr)
            canonical = _run(
                [
                    str(REGEN_MARKDOWN),
                    "--schemas",
                    str(schemas),
                    "--out",
                    str(canonical_out),
                    *default_specs,
                ]
            )
            self.assertEqual(canonical.returncode, 0, canonical.stderr)

            legacy_tree = _tree(legacy_out)
            canonical_tree = _tree(canonical_out)
            self.assertEqual(
                sorted(legacy_tree), sorted(canonical_tree), "file sets differ"
            )
            self.assertEqual(legacy_tree, canonical_tree, "file contents differ")
            self.assertTrue(legacy_tree, "nothing was generated")

    def test_no_arg_discovers_namespace_outside_the_binding_default(self):
        # A namespace that merely has _index.yaml is rendered when no spec
        # names are passed. gen-docs must still refuse to opt into it.
        with tempfile.TemporaryDirectory() as d:
            root = Path(d) / "schemas"
            out = Path(d) / "out"
            legacy_out = Path(d) / "legacy"
            _write_spec(root, "cmis", _memory_map_index("cmis", "Synthetic CMIS"))
            _write_spec(
                root, "extra-ns", _memory_map_index("extra-ns", "Synthetic Extra")
            )

            discovered = _run(
                [str(REGEN_MARKDOWN), "--schemas", str(root), "--out", str(out)]
            )
            self.assertEqual(discovered.returncode, 0, discovered.stderr)
            self.assertTrue((out / "cmis" / "index.md").is_file())
            self.assertTrue((out / "extra-ns" / "index.md").is_file())
            top = (out / "index.md").read_text(encoding="utf-8")
            self.assertIn("[`extra-ns`](extra-ns/index.md)", top)
            self.assertIn(
                "| [`extra-ns`](extra-ns/index.md) | Synthetic Extra | `1.0` | rendered |",
                top,
            )
            self.assertNotIn("no generated output in this tree", top)

            legacy = _run([sys.executable, str(GEN_DOCS), str(root), str(legacy_out)])
            self.assertEqual(legacy.returncode, 0, legacy.stderr)
            self.assertFalse((legacy_out / "extra-ns" / "index.md").exists())


if __name__ == "__main__":
    unittest.main()
