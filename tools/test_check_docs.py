"""Self-test for the documentation checker.

Dependency-free (stdlib unittest). Run:  python3 -m unittest tools.test_check_docs -v
The validator keeps mutable module-level config; every test monkeypatches it, so
CheckDocsTestCase snapshots and restores those globals to prevent cross-class leak
(unittest runs classes in alphabetical order).

Two test layers exercise worktree coverage:
  * module-level tests patch ROOT/REPO_BASES directly (fast, no git needed);
  * CliWorktreeRegressionTest builds a throwaway git repository with a real
    linked worktree and runs the real documentation CLI inside it, so workspace
discovery is exercised exactly as on the dev host (skipped when git is absent).
"""

import importlib.util
import os
import shutil
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

_SPEC = importlib.util.spec_from_file_location(
    "check_docs", Path(__file__).with_name(Path(__file__).stem.removeprefix("test_").replace("_", "-") + ".py"))
checkdocs = importlib.util.module_from_spec(_SPEC)
# Register in sys.modules BEFORE exec so @dataclass can resolve string
# annotations (from __future__ import annotations) via the module namespace.
sys.modules["check_docs"] = checkdocs
_SPEC.loader.exec_module(checkdocs)  # module scope: globals exist before any test

_PATCHABLE = (
    "ROOT", "ACTIVE_SCOPE", "ACTIVE_REPOS", "ACTIVE_PERSONAS",
    "ENTRY_POINTS", "CURRENT_DOC_PREFIXES", "STALE_TOKENS",
    "NAV_BUDGET_DEFAULT", "NAV_BUDGET_OVERRIDES", "MANIFEST",
    "REACH_EXEMPT_PREFIXES", "REPO_BASES", "SNAPSHOT_PREFIXES",
)


class CheckDocsTestCase(unittest.TestCase):
    """Snapshot/restore mutable module globals so test classes can't leak state."""

    def setUp(self):
        self._saved = {n: getattr(checkdocs, n, None) for n in _PATCHABLE}
        # Default to the pre-worktree model (repos = ROOT/<repo> dirs) so the
        # original ROOT-patching tests keep their meaning; tests that exercise
        # the worktree view set REPO_BASES explicitly.
        checkdocs.REPO_BASES = {}

    def tearDown(self):
        for n, v in self._saved.items():
            setattr(checkdocs, n, v)


class CheckLinksTest(CheckDocsTestCase):
    def test_broken_inline_link_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "a.md").write_text("# a\n[x](missing.md)\n", encoding="utf-8")
            checkdocs.ROOT = root
            probs = checkdocs.check_links([root / "a.md"])
            self.assertTrue(any("broken Markdown link" in p.message for p in probs))

    def test_valid_inline_link_ok(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "a.md").write_text("# a\n[x](b.md)\n", encoding="utf-8")
            (root / "b.md").write_text("# b\n", encoding="utf-8")
            checkdocs.ROOT = root
            self.assertEqual(checkdocs.check_links([root / "a.md", root / "b.md"]), [])

    def test_broken_reference_style_link_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "a.md").write_text("# a\n[x][ref]\n\n[ref]: missing.md\n", encoding="utf-8")
            checkdocs.ROOT = root
            probs = checkdocs.check_links([root / "a.md"])
            self.assertTrue(any("reference link" in p.message for p in probs))


class RstRoleFormatTest(CheckDocsTestCase):
    """Ordinary Markdown is not role-capable: :doc:/:ref:-shaped text in .md
    and .3 files is inert prose (the documented format contract gives ':doc:'
    meaning in RST only; the Sphinx build configures source_suffix = '.rst'),
    while the same roles in .rst files stay executable links that must resolve.

    The AUDIT_QUOTATIONS are copied verbatim (byte-for-byte, including the
    arrow/em-dash separators and nested backticks) from the 2026-09-14 fleet
    audit artifacts whose quotations originally produced ten false findings.
    """

    # DOCS-DRIFT-FINDINGS.md:72-76 (\u2192 arrow form) and
    # DOCS-DRIFT-FIXES.md:14-18 (\u2014 em-dash form), in recorded order.
    AUDIT_QUOTATIONS = [
        "- `Documentation/xcvr/index.rst:67` \u2192 `:doc:`configfs``",
        "- `Documentation/xcvr/uapi-stability.rst:128` \u2192 `:doc:`configfs``",
        "- `Documentation/xcvr/arch/overview.rst:122` \u2192 `:doc:`../configfs``",
        "- `Documentation/xcvr/arch/internals/safety.rst:20` \u2192 `:doc:`../../configfs``",
        "- `Documentation/xcvr/cache-architecture.rst:111` \u2192 `:doc:`io_latency``",
        "- `Documentation/xcvr/uapi-stability.rst:128` \u2014 `:doc:`configfs``",
        "- `Documentation/xcvr/arch/internals/safety.rst:20` \u2014 `:doc:`../../configfs``",
        "- `Documentation/xcvr/arch/overview.rst:122` \u2014 `:doc:`../configfs``",
        "- `Documentation/xcvr/index.rst:67` \u2014 `:doc:`configfs``",
        "- `Documentation/xcvr/cache-architecture.rst:111` \u2014 `:doc:`io_latency``",
    ]

    def _probs(self, name: str, text: str):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            f = root / name
            f.write_text(text, encoding="utf-8")
            checkdocs.ROOT = root
            return checkdocs.check_links([f])

    # --- inert roles in ordinary Markdown ---

    def test_markdown_prose_role_produces_no_rst_finding(self):
        self.assertEqual(self._probs("a.md", "# a\n\nUse :doc:`missing` here.\n"), [])

    def test_markdown_inline_code_role_is_inert(self):
        self.assertEqual(
            self._probs("a.md", "# a\n\nSpell it `:doc:`missing``.\n"), [])

    def test_markdown_fenced_role_is_inert(self):
        self.assertEqual(
            self._probs("a.md", "# a\n\n```\n:doc:`missing`\n```\n"), [])

    def test_man_page_role_shaped_text_is_inert(self):
        self.assertEqual(self._probs("a.3", ".TH a\n:doc:`missing`\n"), [])

    def test_audit_quotation_lines_are_inert_in_markdown(self):
        for line in self.AUDIT_QUOTATIONS:
            with self.subTest(quotation=line):
                self.assertEqual(
                    self._probs("findings.md", f"# Drift\n\n{line}\n"), [])

    # --- genuine Markdown defects beside inert roles still fail ---

    def test_broken_markdown_link_beside_inert_role_still_flagged(self):
        probs = self._probs(
            "a.md", "# a\n:doc:`missing-role` [bad](missing.md)\n")
        self.assertEqual(
            [p.message for p in probs], ["broken Markdown link: missing.md"])
        self.assertEqual(probs[0].line, 2)

    def test_broken_reference_link_beside_inert_role_still_flagged(self):
        probs = self._probs(
            "a.md", "# a\n:doc:`missing-role` [bad][r]\n\n[r]: missing-ref.md\n")
        self.assertEqual(
            [p.message for p in probs],
            ["broken reference link [r]: missing-ref.md"])

    def test_valid_markdown_links_beside_inert_role_resolve(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "a.md").write_text(
                "# a\n:doc:`missing` [ok](b.md) and [ok][r]\n\n[r]: b.md\n",
                encoding="utf-8")
            (root / "b.md").write_text("# b\n", encoding="utf-8")
            checkdocs.ROOT = root
            self.assertEqual(
                checkdocs.check_links([root / "a.md", root / "b.md"]), [])

    def test_role_shaped_token_creates_no_reachability_edge(self):
        # A :doc:-shaped token in Markdown must not manufacture a graph edge:
        # its "target" stays an orphan.
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "README.md").write_text("# r\nsee [a](a.md)\n", encoding="utf-8")
            (root / "a.md").write_text("# a\n:doc:`b`\n", encoding="utf-8")
            (root / "b.rst").write_text("B\n=\n", encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.ENTRY_POINTS = [Path("README.md")]
            checkdocs.CURRENT_DOC_PREFIXES = (
                Path("README.md"), Path("a.md"), Path("b.rst"))
            checkdocs.REACH_EXEMPT_PREFIXES = ()
            probs = checkdocs.check_reachability(
                [root / "README.md", root / "a.md", root / "b.rst"])
            self.assertEqual([p.path.name for p in probs], ["b.rst"])

    # --- roles stay executable links in .rst files ---

    def test_rst_broken_doc_role_still_flagged_at_exact_line(self):
        probs = self._probs("a.rst", "Title\n=====\nSee :doc:`missing`.\n")
        self.assertEqual(len(probs), 1)
        self.assertIn("broken RST doc link: missing", probs[0].message)
        self.assertEqual(probs[0].line, 3)
        self.assertEqual(probs[0].path.name, "a.rst")

    def test_rst_broken_doc_role_inside_rst_historical_fence_still_flagged(self):
        # Historical fences exempt superseded names only; they must never
        # suppress a genuine broken RST link inside them.
        probs = self._probs(
            "a.rst",
            "Title\n=====\n.. historical::\nSee :doc:`missing`.\n.. end-historical::\n")
        self.assertEqual(
            [p.message for p in probs], ["broken RST doc link: missing"])
        self.assertEqual(probs[0].line, 4)

    def test_rst_broken_doc_role_inside_html_fence_markers_still_flagged(self):
        # Same property for the Markdown-flavour markers used by the audit
        # artifacts: inert name exemptions, never link exemptions.
        probs = self._probs(
            "a.rst",
            "Title\n=====\n<!-- historical: begin -->\n"
            "See :doc:`missing`.\n<!-- historical: end -->\n")
        self.assertEqual(
            [p.message for p in probs], ["broken RST doc link: missing"])

    def test_rst_relative_and_labeled_doc_roles_resolve(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "a.rst").write_text(
                "Title\n=====\nSee :doc:`b` and :doc:`Label <b>`.\n", encoding="utf-8")
            (root / "b.rst").write_text("B\n=\n", encoding="utf-8")
            (root / "nested").mkdir()
            (root / "nested" / "sub.rst").write_text(
                "Sub\n===\nSee :doc:`../b`.\n", encoding="utf-8")
            checkdocs.ROOT = root
            self.assertEqual(
                checkdocs.check_links(
                    [root / "a.rst", root / "b.rst", root / "nested" / "sub.rst"]),
                [])


class HistoricalMarkerControlTest(CheckDocsTestCase):
    """Marker controls around the .rst-role guard: fences exempt superseded
    names only \u2014 never genuine broken links \u2014 and a token immediately after a
    closed fence still fails at its exact line."""

    def _run_name(self, text):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "docs").mkdir()
            f = root / "docs" / "C.md"
            f.write_text(text, encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.CURRENT_DOC_PREFIXES = (Path("docs"),)
            checkdocs.STALE_TOKENS = ("yanos-xcvr",)
            return checkdocs.check_name_drift([f])

    def test_token_immediately_after_closing_marker_fails_at_exact_line(self):
        probs = self._run_name(
            "<!-- historical: begin -->\nyanos-xcvr\n"
            "<!-- historical: end -->\nyanos-xcvr\n")
        self.assertEqual(len(probs), 1)
        self.assertIn("superseded name", probs[0].message)
        self.assertEqual(probs[0].line, 4)

    def test_fenced_old_name_passes_with_peer_outside(self):
        # The fenced occurrence is exempt; only the outside one is a finding.
        probs = self._run_name(
            "<!-- historical: begin -->\nyanos-xcvr\n"
            "<!-- historical: end -->\ncurrent text\n")
        self.assertEqual(probs, [])

    def test_broken_markdown_link_inside_balanced_historical_block_fails(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "docs").mkdir()
            f = root / "docs" / "C.md"
            f.write_text(
                "<!-- historical: begin -->\n[old](gone.md)\n"
                "<!-- historical: end -->\n",
                encoding="utf-8")
            checkdocs.ROOT = root
            probs = checkdocs.check_links([f])
            self.assertEqual(
                [p.message for p in probs], ["broken Markdown link: gone.md"])


class EntrypointTest(CheckDocsTestCase):
    def test_missing_entrypoint_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            checkdocs.ROOT = root
            checkdocs.ENTRY_POINTS = [Path("README.md")]
            probs = checkdocs.check_entrypoints()
            self.assertTrue(any("missing entrypoint" in p.message for p in probs))

    def test_present_entrypoint_ok(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "README.md").write_text("# r\n", encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.ENTRY_POINTS = [Path("README.md")]
            self.assertEqual(checkdocs.check_entrypoints(), [])


class SizeCapTest(CheckDocsTestCase):
    def test_oversized_nav_file_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "BIG.md").write_text("# t\n" + "x\n" * 200, encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.ENTRY_POINTS = [Path("BIG.md")]
            checkdocs.NAV_BUDGET_DEFAULT = 120
            checkdocs.NAV_BUDGET_OVERRIDES = {}
            probs = checkdocs.check_nav_size()
            self.assertTrue(any("size budget" in p.message for p in probs))


class SelfLocationTest(CheckDocsTestCase):
    def _run(self, text):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "E.md").write_text(text, encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.ENTRY_POINTS = [Path("E.md")]
            return checkdocs.check_self_location()

    def test_missing_nexthop_flagged(self):
        probs = self._run("# Title\nThis repo owns X.\nNo links here.\n")
        self.assertTrue(any("self-location" in p.message for p in probs))

    def test_compliant_passes(self):
        probs = self._run("# Title\nThis repo owns X.\nSee [next](docs/README.md).\n")
        self.assertEqual(probs, [])

    def test_reference_style_link_satisfies_nexthop(self):
        # Reference-style link in first 20 lines with definition later in file
        text = (
            "# Title\n"
            "This repo owns X.\n"
            "See [next][docs].\n"
            "\n" * 30 +
            "[docs]: docs/README.md\n"
        )
        probs = self._run(text)
        self.assertEqual(probs, [])


class NameDriftTest(CheckDocsTestCase):
    def _run(self, text):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "docs").mkdir()
            f = root / "docs" / "C.md"
            f.write_text(text, encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.CURRENT_DOC_PREFIXES = (Path("docs"),)
            checkdocs.STALE_TOKENS = ("yanos-xcvr",)
            return checkdocs.check_name_drift([f])

    def test_bare_flagged(self):
        self.assertTrue(self._run("Use yanos-xcvr now.\n"))

    def test_fenced_ok(self):
        self.assertEqual(
            self._run("<!-- historical: begin -->\nyanos-xcvr\n<!-- historical: end -->\n"), [])

    def test_word_bypass_removed(self):
        # outside a fence -> still flagged even though "legacy" is on the line
        self.assertTrue(self._run("The legacy yanos-xcvr name.\n"))

    def test_unclosed_fence_flagged(self):
        # begin with no end -> must emit an unclosed-fence problem
        probs = self._run("<!-- historical: begin -->\nyanos-xcvr\n")
        self.assertTrue(any("unclosed" in p.message for p in probs))


class ReachabilityTest(CheckDocsTestCase):
    def test_orphan_reported(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "README.md").write_text("# r\nsee [a](a.md)\n", encoding="utf-8")
            (root / "a.md").write_text("# a\n", encoding="utf-8")
            (root / "b.md").write_text("# b\n", encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.ENTRY_POINTS = [Path("README.md")]
            checkdocs.CURRENT_DOC_PREFIXES = (Path("README.md"), Path("a.md"), Path("b.md"))
            checkdocs.REACH_EXEMPT_PREFIXES = ()
            files = [root / "README.md", root / "a.md", root / "b.md"]
            probs = checkdocs.check_reachability(files)
            self.assertTrue(any("orphan" in p.message and p.path.name == "b.md" for p in probs))

    def test_reference_style_link_makes_doc_reachable(self):
        # b.md reachable only via reference-style link -> should not be an orphan
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "README.md").write_text(
                "# r\nsee [b][b-link]\n\n[b-link]: b.md\n", encoding="utf-8")
            (root / "b.md").write_text("# b\n", encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.ENTRY_POINTS = [Path("README.md")]
            checkdocs.CURRENT_DOC_PREFIXES = (Path("README.md"), Path("b.md"))
            checkdocs.REACH_EXEMPT_PREFIXES = ()
            files = [root / "README.md", root / "b.md"]
            probs = checkdocs.check_reachability(files)
            self.assertEqual(probs, [])


class AnchorTest(CheckDocsTestCase):
    def test_dangling_anchor(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "README.md").write_text("# r\n[x](a.md#missing)\n", encoding="utf-8")
            (root / "a.md").write_text("# a\n## Present\n", encoding="utf-8")
            checkdocs.ROOT = root
            files = [root / "README.md", root / "a.md"]
            probs = checkdocs.check_anchors(files)
            self.assertTrue(any("anchor" in p.message for p in probs))

    def test_present_anchor_ok(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "README.md").write_text("# r\n[x](a.md#present)\n", encoding="utf-8")
            (root / "a.md").write_text("# a\n## Present\n", encoding="utf-8")
            checkdocs.ROOT = root
            files = [root / "README.md", root / "a.md"]
            self.assertEqual(checkdocs.check_anchors(files), [])

    def test_duplicate_heading_dedup_variant_ok(self):
        # Second occurrence of ## Usage generates #usage-1; link to that should pass.
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "README.md").write_text("# r\n[x](a.md#usage-1)\n", encoding="utf-8")
            (root / "a.md").write_text("# a\n## Usage\nfirst\n## Usage\nsecond\n", encoding="utf-8")
            checkdocs.ROOT = root
            files = [root / "README.md", root / "a.md"]
            self.assertEqual(checkdocs.check_anchors(files), [])


class ScopeTest(CheckDocsTestCase):
    def test_project_scope_excludes_component_checkouts(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "openxcvr").mkdir()
            (root / "openxcvr" / "README.md").write_text("# project\n", encoding="utf-8")
            (root / "openxcvr-lib").mkdir()
            (root / "openxcvr-lib" / "README.md").write_text("# lib\n", encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.ACTIVE_SCOPE = "project"
            checkdocs.ACTIVE_REPOS = ("openxcvr",)
            files, problems = checkdocs.doc_files()
            self.assertEqual(problems, [])
            self.assertEqual(files, [root / "openxcvr" / "README.md"])

    def test_workspace_scope_includes_only_active_component_repos(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "openxcvr").mkdir()
            (root / "openxcvr" / "README.md").write_text("# project\n", encoding="utf-8")
            (root / "openxcvr-lib").mkdir()
            (root / "openxcvr-lib" / "README.md").write_text("# lib\n", encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.ACTIVE_SCOPE = "workspace"
            checkdocs.ACTIVE_REPOS = ("openxcvr-lib",)
            files, problems = checkdocs.doc_files()
            self.assertEqual(problems, [])
            self.assertEqual(files, [root / "openxcvr-lib" / "README.md"])

    def test_workspace_scope_maps_github_repo_links(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "openxcvr-lib").mkdir()
            (root / "openxcvr-lib" / "README.md").write_text("# lib\n", encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.ACTIVE_SCOPE = "workspace"
            resolved = checkdocs.resolve_markdown_link(
                root / "README.md", "https://github.com/OpenXCVR/openxcvr-lib")
            self.assertEqual(resolved, (root / "openxcvr-lib" / "README.md").resolve())


class PersonaTest(CheckDocsTestCase):
    def test_unreachable_persona_target_flagged(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "README.md").write_text("# r\nno links\n", encoding="utf-8")
            (root / "cli").mkdir()
            (root / "cli" / "README.md").write_text("# cli\n", encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.MANIFEST = {"personas": {
                "operator": {"start": "README.md", "expect": ["cli/README.md"]}}}
            probs = checkdocs.check_personas()
            self.assertTrue(any("persona" in p.message and "operator" in p.message for p in probs))

    def test_reachable_persona_target_ok(self):
        with tempfile.TemporaryDirectory() as d:
            root = Path(d)
            (root / "README.md").write_text("# r\nsee [cli](cli/README.md)\n", encoding="utf-8")
            (root / "cli").mkdir()
            (root / "cli" / "README.md").write_text("# cli\n", encoding="utf-8")
            checkdocs.ROOT = root
            checkdocs.MANIFEST = {"personas": {
                "operator": {"start": "README.md", "expect": ["cli/README.md"]}}}
            self.assertEqual(checkdocs.check_personas(), [])


class WorkspaceDiscoveryTest(unittest.TestCase):
    """resolve_main_repo_root must use git metadata, never a guessed layout."""

    @staticmethod
    def _git(cwd, *args):
        env = dict(os.environ)
        env["GIT_CONFIG_GLOBAL"] = os.devnull
        env["GIT_CONFIG_SYSTEM"] = os.devnull
        return subprocess.run(
            ["git", "-C", str(cwd), *args], env=env,
            capture_output=True, text=True, check=True)

    @unittest.skipIf(shutil.which("git") is None, "git not available")
    def test_linked_worktree_resolves_owning_repository_root(self):
        with tempfile.TemporaryDirectory() as d:
            main = Path(d) / "openxcvr-lib"
            (main / "docs").mkdir(parents=True)
            (main / "README.md").write_text("# lib\n", encoding="utf-8")
            self._git(main, "init", "-q", "-b", "main")
            self._git(main, "add", "-A")
            self._git(main, "-c", "user.name=t", "-c", "user.email=t@example.invalid",
                      "commit", "-q", "-m", "init")
            wt = main / ".worktrees" / "wave"
            self._git(main, "worktree", "add", "-q", "-b", "wave", str(wt))
            self.assertEqual(checkdocs.resolve_main_repo_root(wt), main.resolve())
            self.assertEqual(checkdocs.resolve_main_repo_root(main), main.resolve())

    def test_plain_directory_fails_closed(self):
        with tempfile.TemporaryDirectory() as d:
            # Not a git checkout: discovery must refuse (None), never guess.
            self.assertIsNone(checkdocs.resolve_main_repo_root(Path(d)))


class WorktreeViewTest(CheckDocsTestCase):
    """The workspace view from a linked worktree: this worktree IS the lib repo;
    siblings resolve in the common workspace; .worktrees hides sibling
    worktrees but never the selected one."""

    def setUp(self):
        super().setUp()
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        ws = Path(self._tmp.name)
        self.ws = ws
        self.wt = ws / "openxcvr-lib" / ".worktrees" / "wave"
        (self.wt / "docs" / "reports").mkdir(parents=True)
        (self.wt / "docs" / "plans").mkdir(parents=True)
        (ws / "openxcvr" / "docs").mkdir(parents=True)
        (ws / "openxcvr" / ".worktrees" / "sibling-wt").mkdir(parents=True)
        (self.wt / "README.md").write_text("# lib\n", encoding="utf-8")
        (self.wt / "docs" / "reports" / "README.md").write_text(
            "# Receipts\n\nLibrary-issued receipts.\n\n- [r1](r1.md)\n", encoding="utf-8")
        (self.wt / "docs" / "reports" / "r1.md").write_text("# r1\n", encoding="utf-8")
        (self.wt / "docs" / "plans" / "parked.md").write_text("# parked\n", encoding="utf-8")
        (ws / "openxcvr" / "README.md").write_text("# hub\n", encoding="utf-8")
        (ws / "openxcvr" / "docs" / "README.md").write_text("# docs\n", encoding="utf-8")
        (ws / "openxcvr" / ".worktrees" / "sibling-wt" / "hidden.md").write_text(
            "# hidden\n", encoding="utf-8")
        checkdocs.ROOT = ws
        checkdocs.REPO_BASES = {"openxcvr-lib": self.wt}
        checkdocs.ACTIVE_SCOPE = "workspace"
        checkdocs.ACTIVE_REPOS = ("openxcvr", "openxcvr-lib")
        checkdocs.CURRENT_DOC_PREFIXES = (Path("openxcvr-lib/docs/reports"),)
        checkdocs.ENTRY_POINTS = [Path("openxcvr-lib/docs/reports/README.md")]

    def test_doc_files_scans_selected_worktree_and_sibling_checkouts(self):
        files, problems = checkdocs.doc_files()
        self.assertEqual(problems, [])
        expected = sorted([
            self.wt / "README.md",
            self.wt / "docs" / "reports" / "README.md",
            self.wt / "docs" / "reports" / "r1.md",
            self.wt / "docs" / "plans" / "parked.md",
            self.ws / "openxcvr" / "README.md",
            self.ws / "openxcvr" / "docs" / "README.md",
        ])
        self.assertEqual(files, expected)

    def test_worktrees_exclusion_hides_sibling_worktrees_not_selected_one(self):
        files, problems = checkdocs.doc_files()
        self.assertEqual(problems, [])
        hidden = self.ws / "openxcvr" / ".worktrees" / "sibling-wt" / "hidden.md"
        self.assertNotIn(hidden, files)
        self.assertIn(self.wt / "docs" / "reports" / "r1.md", files)

    def test_plans_records_are_not_current_docs(self):
        self.assertFalse(checkdocs.is_current_doc(
            self.wt / "docs" / "plans" / "parked.md"))
        self.assertTrue(checkdocs.is_current_doc(
            self.wt / "docs" / "reports" / "r1.md"))

    def test_cross_repo_link_from_worktree_resolves_sibling_checkout(self):
        resolved = checkdocs.resolve_markdown_link(
            self.wt / "docs" / "reports" / "r1.md",
            "../../../openxcvr/README.md")
        self.assertEqual(resolved, (self.ws / "openxcvr" / "README.md").resolve())

    def test_github_link_to_owning_repo_maps_to_selected_worktree(self):
        resolved = checkdocs.resolve_markdown_link(
            self.wt / "README.md",
            "https://github.com/OpenXCVR/openxcvr-lib")
        self.assertEqual(resolved, (self.wt / "README.md").resolve())

    def test_entry_point_paths_resolve_into_selected_worktree(self):
        self.assertEqual(
            checkdocs.workspace_path(Path("openxcvr-lib/README.md")),
            self.wt / "README.md")
        self.assertEqual(
            checkdocs.workspace_path(Path("openxcvr/README.md")),
            self.ws / "openxcvr" / "README.md")

    def test_problem_rendering_uses_workspace_logical_paths(self):
        problem = checkdocs.Problem(
            self.wt / "docs" / "reports" / "r1.md", 3, "boom")
        self.assertTrue(problem.render().startswith(
            "openxcvr-lib/docs/reports/r1.md:3:"))

    def test_missing_repo_root_is_reported(self):
        checkdocs.ACTIVE_REPOS = ("sample-driver",)
        probs = checkdocs.check_repo_roots()
        self.assertEqual(len(probs), 1)
        self.assertIn("missing repo root: sample-driver", probs[0].message)

    def test_unreadable_document_is_reported_not_skipped(self):
        if os.geteuid() == 0:
            self.skipTest("running as root: permission bits would not block reads")
        f = self.wt / "docs" / "reports" / "r1.md"
        f.chmod(0o000)
        self.addCleanup(f.chmod, 0o644)
        probs = checkdocs.check_unreadable([f])
        self.assertEqual(len(probs), 1)
        self.assertIn("unreadable document", probs[0].message)


class _FailingScandir:
    """os.scandir replacement that raises for selected directories.

    Lets traversal failures be exercised deterministically, without depending on
    permission bits (which a root user would bypass).
    """

    def __init__(self, failing):
        self.failing = {Path(p).resolve() for p in failing}
        self.calls: list[Path] = []

    def __call__(self, path):
        path = Path(path)
        self.calls.append(path)
        if path.resolve() in self.failing:
            raise PermissionError(13, "Permission denied", str(path))
        return os.scandir(path)


class TraversalErrorTest(CheckDocsTestCase):
    """An unreadable in-scope subtree is a finding, never a silent skip: the
    rest of that repository and the other repositories stay in the scan."""

    def setUp(self):
        super().setUp()
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.ws = Path(self._tmp.name) / "ws"
        self.hub = self.ws / "openxcvr"
        self.lib = self.ws / "openxcvr-lib"
        (self.hub / "docs").mkdir(parents=True)
        (self.lib / "docs" / "reports").mkdir(parents=True)
        (self.lib / "docs" / "notes").mkdir(parents=True)
        (self.lib / "docs" / "sub").mkdir(parents=True)
        (self.lib / "docs" / "broken").mkdir(parents=True)
        (self.hub / "README.md").write_text("# hub\n", encoding="utf-8")
        (self.hub / "docs" / "README.md").write_text("# hub docs\n", encoding="utf-8")
        (self.lib / "README.md").write_text("# lib\n", encoding="utf-8")
        (self.lib / "docs" / "reports" / "r1.md").write_text("# r1\n", encoding="utf-8")
        (self.lib / "docs" / "notes" / "a.md").write_text("# a\n", encoding="utf-8")
        (self.lib / "docs" / "sub" / "c.md").write_text("# c\n", encoding="utf-8")
        (self.lib / "docs" / "broken" / "hidden.md").write_text("# hidden\n", encoding="utf-8")
        checkdocs.ROOT = self.ws
        checkdocs.ACTIVE_SCOPE = "workspace"
        checkdocs.ACTIVE_REPOS = ("openxcvr", "openxcvr-lib")

    def test_unreadable_subtree_is_reported_and_readable_peers_kept(self):
        scandir = _FailingScandir([self.lib / "docs" / "broken"])
        files, problems = checkdocs.doc_files(scandir)
        # The unreadable directory's document is not silently dropped from the
        # verdict: it is a finding. Everything readable is still validated.
        self.assertEqual(len(problems), 1)
        self.assertIn("unreadable subtree", problems[0].message)
        self.assertIn("Permission denied", problems[0].message)
        self.assertEqual(problems[0].path, self.lib / "docs" / "broken")
        self.assertTrue(problems[0].render().startswith("openxcvr-lib/docs/broken:0:"))
        self.assertEqual(files, sorted([
            self.hub / "README.md",
            self.hub / "docs" / "README.md",
            self.lib / "README.md",
            self.lib / "docs" / "reports" / "r1.md",
            self.lib / "docs" / "notes" / "a.md",
            self.lib / "docs" / "sub" / "c.md",
        ]))

    def test_unreadable_subtree_does_not_drop_its_whole_repository(self):
        scandir = _FailingScandir([self.lib / "docs"])
        files, problems = checkdocs.doc_files(scandir)
        self.assertEqual([p.path for p in problems], [self.lib / "docs"])
        # The lib repo keeps its other documents; only the failed subtree is out.
        self.assertIn(self.lib / "README.md", files)
        self.assertIn(self.hub / "README.md", files)
        self.assertFalse([f for f in files if self.lib / "docs" in f.parents])

    def test_unreadable_repo_root_is_reported_and_other_repos_scanned(self):
        scandir = _FailingScandir([self.hub])
        files, problems = checkdocs.doc_files(scandir)
        self.assertEqual([p.path for p in problems], [self.hub])
        self.assertIn("unreadable subtree", problems[0].message)
        self.assertIn(self.lib / "README.md", files)

    def test_excluded_directories_are_pruned_not_reported(self):
        excluded = [self.lib / "node_modules", self.lib / ".worktrees" / "other",
                    self.lib / "target", self.lib / "docs" / "__pycache__"]
        for directory in excluded:
            directory.mkdir(parents=True)
            (directory / "skipped.md").write_text("# skipped\n", encoding="utf-8")
        scandir = _FailingScandir(excluded)
        files, problems = checkdocs.doc_files(scandir)
        # Excluded trees are out of scope by construction: pruned before any
        # listing, so neither their documents nor an unreadable error appear.
        self.assertEqual(problems, [])
        self.assertFalse([f for f in files if f.name == "skipped.md"])
        called = {Path(c).resolve() for c in scandir.calls}
        for directory in excluded:
            self.assertNotIn(directory.resolve(), called)

    def test_declared_snapshot_is_out_of_scope_but_its_neighbours_are_not(self):
        # A pinned copy of another repository's docs carries links written for
        # that repository. Only the declared path is excluded: a project file
        # under another directory of the same name is still scanned.
        snapshot = self.lib / "vendor" / "upstream" / "copy"
        beside = self.lib / "vendor" / "upstream"
        own = self.lib / "tests" / "vendor"
        for directory in (snapshot, own):
            directory.mkdir(parents=True)
            (directory / "README.md").write_text("# copy\n[x](../../gone.md)\n", encoding="utf-8")
        (beside / "NOTES.md").write_text("# notes\n[x](gone.md)\n", encoding="utf-8")
        (snapshot / "sub").mkdir()
        checkdocs.SNAPSHOT_PREFIXES = (Path("openxcvr-lib/vendor/upstream/copy"),)
        # The snapshot is never listed: an unreadable directory inside it is
        # not an input, so it cannot make the run inconclusive.
        scandir = _FailingScandir([snapshot, snapshot / "sub"])
        files, problems = checkdocs.doc_files(scandir)
        self.assertEqual(problems, [])
        self.assertNotIn(snapshot.resolve(), {Path(c).resolve() for c in scandir.calls})
        self.assertNotIn(snapshot / "README.md", files)
        self.assertIn(own / "README.md", files)
        self.assertIn(beside / "NOTES.md", files)
        broken = checkdocs.check_links(files)
        self.assertEqual(sorted(p.path for p in broken), sorted([beside / "NOTES.md", own / "README.md"]))

    def test_problems_are_deterministically_ordered(self):
        scandir = _FailingScandir([self.lib / "docs" / "broken", self.hub])
        _, problems = checkdocs.doc_files(scandir)
        self.assertEqual([p.path for p in problems], sorted([self.hub, self.lib / "docs" / "broken"], key=str))

    def test_unreadable_subtree_causes_inputs_incomplete(self):
        scandir = _FailingScandir([self.lib / "docs" / "broken"])
        files, traversal_probs = checkdocs.doc_files(scandir)
        root_probs = checkdocs.check_repo_roots()
        read_probs = checkdocs.check_unreadable(files)
        inputs_complete = not bool(root_probs or traversal_probs or read_probs)
        self.assertFalse(inputs_complete)


class DocSymlinkTest(CheckDocsTestCase):
    """Candidate documentation collection must handle symlinks properly:
    dangling symlinks are collected so check_unreadable fails closed, healthy
    symlinks are validated, loops fail closed, and directory symlinks are never
    recursed."""

    def setUp(self):
        super().setUp()
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        self.ws = Path(self._tmp.name) / "ws"
        self.lib = self.ws / "openxcvr-lib"
        (self.lib / "docs").mkdir(parents=True)
        (self.lib / "README.md").write_text("# lib\n", encoding="utf-8")
        (self.lib / "docs" / "ok.md").write_text("# ok\n", encoding="utf-8")
        checkdocs.ROOT = self.ws
        checkdocs.ACTIVE_SCOPE = "workspace"
        checkdocs.ACTIVE_REPOS = ("openxcvr-lib",)

    def test_dangling_doc_symlinks_collected_and_reported_with_readable_peer(self):
        # Dangling .md, .rst, .3 symlinks must be collected into files so
        # check_unreadable classifies them as inconclusive findings rather
        # than silently vanishing when readable peers exist.
        dangling_md = self.lib / "docs" / "dangling.md"
        dangling_rst = self.lib / "docs" / "dangling.rst"
        dangling_3 = self.lib / "docs" / "dangling.3"
        dangling_md.symlink_to(self.lib / "docs" / "nonexistent.md")
        dangling_rst.symlink_to(self.lib / "docs" / "nonexistent.rst")
        dangling_3.symlink_to(self.lib / "docs" / "nonexistent.3")

        files, problems = checkdocs.doc_files()
        self.assertEqual(problems, [])
        self.assertIn(dangling_md, files)
        self.assertIn(dangling_rst, files)
        self.assertIn(dangling_3, files)
        self.assertIn(self.lib / "docs" / "ok.md", files)

        read_probs = checkdocs.check_unreadable(files)
        read_prob_paths = {p.path for p in read_probs}
        self.assertIn(dangling_md, read_prob_paths)
        self.assertIn(dangling_rst, read_prob_paths)
        self.assertIn(dangling_3, read_prob_paths)
        for p in read_probs:
            if p.path in (dangling_md, dangling_rst, dangling_3):
                self.assertIn("unreadable document", p.message)

    def test_healthy_doc_symlink_collected_and_readable(self):
        target = self.lib / "docs" / "target.md"
        target.write_text("# target\n", encoding="utf-8")
        sym = self.lib / "docs" / "sym.md"
        sym.symlink_to(target)

        files, problems = checkdocs.doc_files()
        self.assertEqual(problems, [])
        self.assertIn(sym, files)
        self.assertEqual(checkdocs.check_unreadable(files), [])

    def test_symlink_loop_reports_unreadable(self):
        loop = self.lib / "docs" / "loop.md"
        loop.symlink_to(loop)

        files, problems = checkdocs.doc_files()
        # Either captured during scan or collected as doc candidate and caught
        # as unreadable document; in neither case may it silently vanish.
        read_probs = checkdocs.check_unreadable(files)
        all_probs = problems + read_probs
        self.assertTrue(any(p.path == loop for p in all_probs))
        for p in all_probs:
            if p.path == loop:
                self.assertTrue("unreadable" in p.message)

    def test_directory_symlinks_are_not_recursed(self):
        outside = Path(self._tmp.name) / "outside"
        outside.mkdir()
        (outside / "outside.md").write_text("# outside\n", encoding="utf-8")
        (self.lib / "docs" / "symdir").symlink_to(outside)

        files, problems = checkdocs.doc_files()
        self.assertEqual(problems, [])
        self.assertNotIn(outside / "outside.md", files)
        self.assertFalse(any("outside.md" in str(f) for f in files))


class OutsideWorkspaceWorktreeTest(CheckDocsTestCase):
    """A linked worktree may live outside the workspace root (scratch paths are
    allowed); REPO_BASES maps it to its repository, so classification, rendering
    and cross-repo link resolution stay workspace-logical."""

    def setUp(self):
        super().setUp()
        self._tmp = tempfile.TemporaryDirectory()
        self.addCleanup(self._tmp.cleanup)
        tmp = Path(self._tmp.name)
        self.ws = tmp / "ws"
        self.lib_main = self.ws / "openxcvr-lib"
        self.wt = tmp / "elsewhere" / "openxcvr-lib-wave"
        (self.ws / "openxcvr" / "docs").mkdir(parents=True)
        (self.lib_main / "docs").mkdir(parents=True)
        (self.wt / "docs" / "reports").mkdir(parents=True)
        (self.ws / "openxcvr" / "README.md").write_text("# hub\n", encoding="utf-8")
        (self.wt / "README.md").write_text("# lib wt\n", encoding="utf-8")
        (self.wt / "docs" / "reports" / "r1.md").write_text("# r1\n", encoding="utf-8")
        checkdocs.ROOT = self.ws
        checkdocs.REPO_BASES = {"openxcvr-lib": self.wt}
        checkdocs.ACTIVE_SCOPE = "workspace"
        checkdocs.ACTIVE_REPOS = ("openxcvr", "openxcvr-lib")
        checkdocs.CURRENT_DOC_PREFIXES = (
            Path("openxcvr-lib/README.md"),
            Path("openxcvr-lib/docs/reports"),
        )

    def test_outside_worktree_paths_stay_workspace_logical(self):
        receipt = self.wt / "docs" / "reports" / "r1.md"
        self.assertEqual(
            checkdocs._logical(receipt), Path("openxcvr-lib/docs/reports/r1.md"))
        self.assertTrue(checkdocs.is_current_doc(receipt))
        self.assertTrue(checkdocs.is_current_doc(self.wt / "README.md"))
        problem = checkdocs.Problem(receipt, 0, "boom")
        self.assertTrue(problem.render().startswith("openxcvr-lib/docs/reports/r1.md:0:"))

    def test_outside_worktree_is_scanned_and_not_excluded(self):
        files, problems = checkdocs.doc_files()
        self.assertEqual(problems, [])
        self.assertEqual(files, sorted([
            self.ws / "openxcvr" / "README.md",
            self.wt / "README.md",
            self.wt / "docs" / "reports" / "r1.md",
        ]))

    def test_cross_repo_link_from_outside_worktree_resolves_sibling(self):
        resolved = checkdocs.resolve_markdown_link(
            self.wt / "docs" / "reports" / "r1.md",
            "../../../openxcvr/README.md")
        self.assertEqual(resolved, (self.ws / "openxcvr" / "README.md").resolve())


class InventoryHeaderTest(CheckDocsTestCase):
    """The generated inventory header must name the scope that was selected."""

    def _header(self, scope: str) -> str:
        checkdocs.ACTIVE_SCOPE = scope
        text = checkdocs.inventory([], [], [], [], [])
        return text.splitlines()[2]

    def test_header_names_workspace_and_project_selectors(self):
        self.assertIn("--workspace --write-inventory", self._header("workspace"))
        self.assertIn("--project --write-inventory", self._header("project"))

    def test_header_names_named_scope_instead_of_workspace(self):
        header = self._header("reference")
        self.assertIn("--scope reference --write-inventory", header)
        self.assertNotIn("--workspace", header)



_TOOL_SRC = Path(__file__).resolve().parent / (Path(_SPEC.origin).name)

_MANIFEST_HEADER = """\
[entry_points]
required = [
{entries}]

[current_doc_prefixes]
prefixes = [
{prefixes}]

[reachability]
exempt_prefixes = []

[nav_budgets]
default = 140

[canonical_names]
current = ["OpenXCVR"]

[canonical_names.superseded]
"libxcvr" = "renamed to libopenxcvr"

[scopes.workspace]
repos = ["openxcvr", "openxcvr-lib"]

[scopes.project]
repos = ["openxcvr"]
entry_points = [
  "openxcvr/README.md",
  "openxcvr/docs/README.md",
]
current_doc_prefixes = [
  "openxcvr/README.md",
  "openxcvr/docs",
]

[scopes.reference]
repos = ["openxcvr-lib"]
entry_points = ["openxcvr-lib/docs/reference/README.md"]
current_doc_prefixes = ["openxcvr-lib/docs/reference"]
"""


def _fixture_manifest(hub_docs: bool) -> str:
    """A minimal valid manifest; hub_docs=False keeps the hub repo in the
    workspace scope while removing it from entry points and current prefixes
    (for the missing-root fail-closed test)."""
    entries = "  \"openxcvr-lib/README.md\",\n  \"openxcvr-lib/docs/reports/README.md\",\n"
    prefixes = "  \"openxcvr-lib/README.md\",\n  \"openxcvr-lib/docs/reports\",\n"
    if hub_docs:
        entries = "  \"openxcvr/README.md\",\n" + entries
        prefixes = "  \"openxcvr/README.md\",\n  \"openxcvr/docs\",\n" + prefixes
    return _MANIFEST_HEADER.format(entries=entries, prefixes=prefixes)


# Unit tests use only synthetic configuration, not the maintainer's live manifest.
with tempfile.TemporaryDirectory() as _config_tmp:
    _config = Path(_config_tmp) / "fixture-manifest.toml"
    _config.write_text(_fixture_manifest(True) + '\n[workspace]\nrepos = ["openxcvr", "openxcvr-lib", "sample-driver", "sample-package"]\narchive_parts = [".archive", "archive", "history", "plans", "status", "superpowers"]\n')
    checkdocs.initialize(_config)
# Used only to label generated inventory; its content has already been loaded.
checkdocs.MANIFEST_PATH = checkdocs.TOOL_REPO_ROOT / "docs/fixture-manifest.toml"


class CliWorktreeRegressionTest(unittest.TestCase):
    """End-to-end CLI regression: the real validator, run from a real linked
    git worktree inside a throwaway workspace. Builds fixtures with git so
    workspace discovery is exercised exactly as on the dev host."""

    @unittest.skipIf(shutil.which("git") is None, "git not available")
    def _build_fixture(self, tmp: Path, worktree_outside: bool = False) -> dict:
        env = dict(os.environ)
        env["GIT_CONFIG_GLOBAL"] = os.devnull
        env["GIT_CONFIG_SYSTEM"] = os.devnull

        def git(cwd, *args):
            subprocess.run(["git", "-C", str(cwd), *args], env=env,
                           capture_output=True, text=True, check=True)

        ws = tmp / "ws"
        hub = ws / "openxcvr"
        lib = ws / "openxcvr-lib"
        (hub / "docs").mkdir(parents=True)
        (hub / ".worktrees" / "other").mkdir(parents=True)
        (hub / "README.md").write_text(
            "# Hub\n\nUnified OpenXCVR project docs.\n\nSee [docs](docs/README.md).\n",
            encoding="utf-8")
        (hub / "docs" / "README.md").write_text(
            "# Docs\n\nProject documentation index.\n\nBack to [README](../README.md).\n",
            encoding="utf-8")
        # Must never be validated: it sits under a sibling repo's .worktrees.
        (hub / ".worktrees" / "other" / "hidden.md").write_text("# hidden\n", encoding="utf-8")

        (lib / "tools").mkdir(parents=True)
        (lib / "docs" / "reports").mkdir(parents=True)
        (lib / "README.md").write_text(
            "# openxcvr-lib\n\nLibrary implementation and validation tooling.\n\n"
            "See [receipts](docs/reports/README.md).\n", encoding="utf-8")
        (lib / "docs" / "fixture-manifest.toml").write_text(
            _fixture_manifest(hub_docs=True), encoding="utf-8")
        (lib / "docs" / "reports" / "README.md").write_text(
            "# Receipts\n\nLibrary-issued validation receipts.\n\n"
            "- [r1](r1.md)\n- [r2](r2.md)\n- [vendor](vendor.md)\n", encoding="utf-8")
        (lib / "docs" / "reports" / "r1.md").write_text(
            "# r1\n\nReceipt one. See [r2](r2.md).\n", encoding="utf-8")
        (lib / "docs" / "reports" / "r2.md").write_text("# r2\n\nReceipt two.\n", encoding="utf-8")
        (lib / "docs" / "reports" / "vendor.md").write_text(
            "# vendor\n\nVendor programming seam. See [r1](r1.md).\n", encoding="utf-8")
        shutil.copyfile(_TOOL_SRC, lib / "tools" / (Path(_SPEC.origin).name))
        git(lib, "init", "-q", "-b", "main")
        git(lib, "add", "-A")
        git(lib, "-c", "user.name=t", "-c", "user.email=t@example.invalid",
            "commit", "-q", "-m", "fixture")
        wt = tmp / "outside-wt" if worktree_outside else lib / ".worktrees" / "wave"
        git(lib, "worktree", "add", "-q", "-b", "wave", str(wt))
        return {"ws": ws, "hub": hub, "lib": lib, "wt": wt}

    @staticmethod
    def _run_cli(cwd: Path, *args):
        return subprocess.run(
            [sys.executable, str(cwd / "tools" / (Path(_SPEC.origin).name)), *args],
            cwd=cwd, capture_output=True, text=True)

    def _mount_fixture(self, tmp: Path) -> dict:
        fx = self._build_fixture(tmp)
        source = fx["hub"] / "lib" / "notes" / "index.md"
        source.parent.mkdir(parents=True)
        source.write_text("# Index\n[crate](../crates/x/README.md)\n", encoding="utf-8")
        target = fx["wt"] / "crates" / "x" / "README.md"
        target.parent.mkdir(parents=True)
        target.write_text("# Crate\n", encoding="utf-8")
        fx.update(source=source, target=target,
                  manifest=fx["wt"] / "docs" / "fixture-manifest.toml")
        return fx

    @staticmethod
    def _declare_mounts(fx, extra=""):
        with fx["manifest"].open("a", encoding="utf-8") as fh:
            fh.write(
                '\n[mounts]\nlinks = [\n'
                '  { path = "openxcvr-lib/notes", source = "openxcvr/lib/notes" },\n'
                + extra + ']\n')

    def test_project_positions_mounted_source_without_library_symlinks(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._mount_fixture(Path(d))
            # Absent mounts retain the old coordinator-relative finding.
            before = self._run_cli(fx["wt"], "--project")
            self.assertEqual(before.returncode, 1)
            self.assertIn("openxcvr/lib/notes/index.md:2: broken Markdown link", before.stderr)
            self._declare_mounts(fx)
            result = self._run_cli(fx["wt"], "--project")
            self.assertEqual(result.returncode, 0, result.stderr)
            self.assertIn("OK: checked 3 documentation files (project)", result.stdout)
            fx["target"].unlink()
            broken = self._run_cli(fx["wt"], "--project")
            self.assertEqual(broken.returncode, 1)
            self.assertEqual(broken.stderr.count("broken Markdown link"), 1)
            self.assertIn("openxcvr-lib/notes/index.md:2: broken Markdown link", broken.stderr)

    def test_link_between_mounts_without_library_symlinks(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._mount_fixture(Path(d))
            note = fx["hub"] / "lib" / "docs" / "note.md"
            note.parent.mkdir()
            note.write_text("# Note\n[index](../notes/index.md)\n", encoding="utf-8")
            fx["source"].write_text("# Index\n[note](../docs/note.md)\n", encoding="utf-8")
            self._declare_mounts(fx,
                '  { path = "openxcvr-lib/docs/note.md", source = "openxcvr/lib/docs/note.md" },\n')
            result = self._run_cli(fx["wt"], "--project")
            self.assertEqual(result.returncode, 0, result.stderr)
            note.unlink()
            broken = self._run_cli(fx["wt"], "--project")
            self.assertEqual(broken.returncode, 1)
            self.assertIn("openxcvr-lib/notes/index.md:2: broken Markdown link", broken.stderr)

    def test_link_from_coordinator_into_mount_without_library_symlinks(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._mount_fixture(Path(d))
            self._declare_mounts(fx)
            readme = fx["hub"] / "docs" / "README.md"
            with readme.open("a", encoding="utf-8") as fh:
                fh.write("[index](../../openxcvr-lib/notes/index.md)\n")
            result = self._run_cli(fx["wt"], "--project")
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_source_parent_peer_keeps_coordinator_position(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._mount_fixture(Path(d))
            self._declare_mounts(fx)
            peer = fx["hub"] / "lib" / "README.md"
            peer.write_text("# Peer\n[docs](../docs/README.md)\n", encoding="utf-8")
            result = self._run_cli(fx["wt"], "--project")
            self.assertEqual(result.returncode, 0, result.stderr)
            peer.write_text("# Peer\n[bad](missing.md)\n", encoding="utf-8")
            broken = self._run_cli(fx["wt"], "--project")
            self.assertIn("openxcvr/lib/README.md:2: broken Markdown link", broken.stderr)

    def test_workspace_deduplicates_mounted_symlinked_file(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._mount_fixture(Path(d))
            source = fx["hub"] / "lib" / "note.md"
            source.write_text("# Note\n[bad](missing.md)\n", encoding="utf-8")
            (fx["wt"] / "note.md").symlink_to(source)
            self._declare_mounts(fx,
                '  { path = "openxcvr-lib/note.md", source = "openxcvr/lib/note.md" },\n')
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 1)
            self.assertEqual(result.stderr.count("broken Markdown link"), 1, result.stderr)
            self.assertIn("openxcvr-lib/note.md:2: broken Markdown link", result.stderr)

    def test_missing_mount_source_makes_inputs_incomplete_without_remapping(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._mount_fixture(Path(d))
            self._declare_mounts(fx,
                '  { path = "openxcvr-lib/missing", source = "openxcvr/lib/missing" },\n')
            # A missing source must not remap/exclude a real file at its mount path.
            (fx["wt"] / "missing").mkdir()
            (fx["wt"] / "missing" / "bad.md").write_text("[bad](absent.md)\n", encoding="utf-8")
            result = self._run_cli(fx["wt"], "--workspace", "--write-inventory")
            self.assertEqual(result.returncode, 1)
            self.assertIn("missing mount source", result.stderr)
            self.assertIn("openxcvr-lib/missing", result.stderr)
            self.assertIn("openxcvr/lib/missing", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("openxcvr-lib/missing/bad.md:1: broken Markdown link", result.stderr)
            self.assertFalse((fx["hub"] / "docs" / "inventory.md").exists())

    def test_mount_manifest_shape_errors_have_manifest_diagnostic(self):
        cases = [
            'mounts = []',
            '[mounts]\nlinks = "not a list"',
            '[mounts]\nlinks = ["not a table"]',
            '[mounts]\nlinks = [{ path = "openxcvr-lib/notes" }]',
            '[mounts]\nlinks = [{ source = "openxcvr/lib/notes" }]',
            '[mounts]\nlinks = [{ path = "openxcvr-lib/notes", source = "openxcvr/lib/notes", extra = true }]',
            '[mounts]\nlinks = [{ path = 1, source = "openxcvr/lib/notes" }]',
            '[mounts]\nlinks = [{ path = "openxcvr-lib/notes", source = 1 }]',
            '[mounts]\nlinks = [{ path = "notes", source = "openxcvr/lib/notes" }]',
            '[mounts]\nlinks = [{ path = "openxcvr-lib/notes", source = "lib" }]',
            '[mounts]\nlinks = [{ path = "openxcvr-lib/notes", source = "openxcvr/lib/notes" },'
            ' { path = "openxcvr-lib/notes", source = "openxcvr/lib/other" }]',
            '[mounts]\nlinks = [{ path = "openxcvr-lib/notes", source = "openxcvr/lib/notes" },'
            ' { path = "openxcvr-lib/other", source = "openxcvr/lib/notes" }]',
            '[mounts]\nlinks = [{ path = "openxcvr-lib/notes", source = "openxcvr/lib/notes" },'
            ' { path = "openxcvr-lib/notes/sub", source = "openxcvr/lib/other" }]',
            '[mounts]\nlinks = [{ path = "openxcvr-lib/notes", source = "openxcvr/lib/notes" },'
            ' { path = "openxcvr-lib/other", source = "openxcvr/lib/notes/sub" }]',
        ]
        with tempfile.TemporaryDirectory() as d:
            fx = self._mount_fixture(Path(d))
            original = fx["manifest"].read_text(encoding="utf-8")
            for shape in cases:
                with self.subTest(shape=shape):
                    # Prefix scalar cases so they remain top-level TOML keys.
                    fx["manifest"].write_text(shape + "\n" + original if shape.startswith("mounts =")
                                              else original + "\n" + shape, encoding="utf-8")
                    result = self._run_cli(fx["wt"], "--project")
                    self.assertNotEqual(result.returncode, 0)
                    self.assertTrue(result.stderr.startswith("--manifest:"), result.stderr)

    def test_scope_of_the_linked_repository_scans_the_mount_source(self):
        # Only the repository the material is linked into is active, and it
        # has no symlinks: the source is still scanned at its linked position.
        with tempfile.TemporaryDirectory() as d:
            fx = self._mount_fixture(Path(d))
            self._declare_mounts(fx)
            with fx["manifest"].open("a", encoding="utf-8") as fh:
                fh.write('\n[scopes.library]\nrepos = ["openxcvr-lib"]\n'
                         'entry_points = ["openxcvr-lib/README.md"]\n'
                         'current_doc_prefixes = ["openxcvr-lib/README.md"]\n'
                         '[scopes.library.personas]\n')
            result = self._run_cli(fx["wt"], "--scope", "library")
            self.assertEqual(result.returncode, 0, result.stderr)
            fx["target"].unlink()
            broken = self._run_cli(fx["wt"], "--scope", "library")
            self.assertEqual(broken.returncode, 1)
            self.assertIn("openxcvr-lib/notes/index.md:2: broken Markdown link", broken.stderr)

    def test_github_url_into_a_mount_is_reported_with_or_without_symlinks(self):
        # Linked material is not in the repository on GitHub, so a URL to it is
        # broken for readers whether or not this checkout has the symlink.
        url = "https://github.com/OpenXCVR/openxcvr-lib/blob/main/notes/index.md"
        with tempfile.TemporaryDirectory() as d:
            fx = self._mount_fixture(Path(d))
            self._declare_mounts(fx)
            readme = fx["hub"] / "docs" / "README.md"
            with readme.open("a", encoding="utf-8") as fh:
                fh.write(f"[published index]({url})\n")
            unlinked = self._run_cli(fx["wt"], "--workspace")
            self.assertIn(f"broken Markdown link: {url}", unlinked.stderr)
            (fx["wt"] / "notes").symlink_to(fx["source"].parent)
            linked = self._run_cli(fx["wt"], "--workspace")
            self.assertIn(f"broken Markdown link: {url}", linked.stderr)

    def test_entrypoint_and_persona_start_at_mount_path(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._mount_fixture(Path(d))
            fx["source"].write_text(
                "# Index\n\nLibrary documentation.\n\nSee [crate](../crates/x/README.md).\n",
                encoding="utf-8")
            self._declare_mounts(fx)
            with fx["manifest"].open("a", encoding="utf-8") as fh:
                fh.write(
                    '\n[scopes.mounted]\nrepos = ["openxcvr"]\n'
                    'entry_points = ["openxcvr-lib/notes/index.md"]\n'
                    'current_doc_prefixes = ["openxcvr-lib/notes"]\n'
                    '[scopes.mounted.personas]\n'
                    'reader = { start = "openxcvr-lib/notes/index.md", '
                    'expect = ["openxcvr-lib/crates/x/README.md"] }\n')
            result = self._run_cli(fx["wt"], "--scope", "mounted")
            self.assertEqual(result.returncode, 0, result.stderr)

    def test_project_from_worktree_validates_sibling_hub(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            result = self._run_cli(fx["wt"], "--project")
            self.assertEqual(
                result.returncode, 0,
                msg=f"stdout={result.stdout!r} stderr={result.stderr!r}")
            self.assertIn("OK: checked 2 documentation files (project)", result.stdout)

    def test_workspace_from_worktree_validates_worktree_and_siblings(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            wt = fx["wt"]
            # A doc that exists only in this worktree: if the validator scanned
            # the main checkout, or excluded the worktree under .worktrees, the
            # count would differ.
            (wt / "docs" / "reports" / "wave-only.md").write_text(
                "# wave\n\nWorktree-only doc. See [r1](r1.md).\n", encoding="utf-8")
            readme = wt / "docs" / "reports" / "README.md"
            readme.write_text(
                readme.read_text(encoding="utf-8") + "- [wave](wave-only.md)\n",
                encoding="utf-8")
            result = self._run_cli(wt, "--workspace")
            self.assertEqual(
                result.returncode, 0,
                msg=f"stdout={result.stdout!r} stderr={result.stderr!r}")
            # 2 hub docs + 6 worktree lib docs (README, receipts README, r1,
            # r2, vendor, wave-only); the sibling .worktrees doc is excluded.
            self.assertIn("OK: checked 8 documentation files (workspace)", result.stdout)

    def test_missing_sibling_root_fails_closed_from_worktree_and_main(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            # Hub repo stays in the workspace scope but loses its entry-point
            # and current-doc registrations, so a silent skip would pass clean.
            variant = _fixture_manifest(hub_docs=False)
            (fx["wt"] / "docs" / "fixture-manifest.toml").write_text(variant, encoding="utf-8")
            (fx["lib"] / "docs" / "fixture-manifest.toml").write_text(variant, encoding="utf-8")
            shutil.rmtree(fx["hub"])
            for cwd in (fx["wt"], fx["lib"]):
                result = self._run_cli(cwd, "--workspace")
                self.assertNotEqual(
                    result.returncode, 0,
                    msg=f"{cwd}: missing repo root passed clean; "
                        f"stdout={result.stdout!r}")
                self.assertIn("missing repo root: openxcvr", result.stderr)

    def test_broken_link_in_receipt_fails(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            r1 = fx["wt"] / "docs" / "reports" / "r1.md"
            r1.write_text(
                r1.read_text(encoding="utf-8") + "\n[broken](nope.md)\n", encoding="utf-8")
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 1)
            self.assertIn("broken Markdown link", result.stderr)
            self.assertIn("openxcvr-lib/docs/reports/r1.md", result.stderr)

    def test_broken_rst_doc_link_in_linked_rst_document_fails(self):
        # End to end: a .rst document linked from the receipts entry point with
        # a missing :doc: target must force a nonzero exit with the exact
        # finding, target and line.
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            receipts = fx["wt"] / "docs" / "reports"
            readme = receipts / "README.md"
            readme.write_text(
                readme.read_text(encoding="utf-8")
                + "- [page](page.rst)\n- [other](other.rst)\n", encoding="utf-8")
            (receipts / "page.rst").write_text(
                "Page\n====\nSee :doc:`missing`.\n", encoding="utf-8")
            (receipts / "other.rst").write_text(
                "Other\n=====\nSee :doc:`page`.\n", encoding="utf-8")
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 1)
            self.assertIn("broken RST doc link: missing", result.stderr)
            self.assertIn("openxcvr-lib/docs/reports/page.rst:3", result.stderr)

    def test_markdown_receipt_with_inert_role_and_genuine_broken_link(self):
        # A Markdown receipt quoting an RST role in prose exactly as the
        # 2026-09-14 audit artifacts do, plus one genuine broken Markdown
        # link: the run fails on the genuine defect and reports no RST finding.
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            r1 = fx["wt"] / "docs" / "reports" / "r1.md"
            r1.write_text(
                r1.read_text(encoding="utf-8")
                + "\n- `Documentation/xcvr/index.rst:67` \u2192 `:doc:`configfs``\n"
                + "\n[broken](nope.md)\n", encoding="utf-8")
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 1)
            self.assertIn("broken Markdown link: nope.md", result.stderr)
            self.assertNotIn("broken RST doc link", result.stderr)

    def test_markdown_receipt_with_inert_role_alone_passes(self):
        # Removing every genuine defect leaves inert role-shaped quotations in
        # ordinary Markdown: the run must pass clean.
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            r1 = fx["wt"] / "docs" / "reports" / "r1.md"
            r1.write_text(
                r1.read_text(encoding="utf-8")
                + "\nQuoted: `:doc:`configfs`` and :doc:`missing` in prose.\n",
                encoding="utf-8")
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 0, msg=result.stderr)

    def test_unreadable_receipt_fails_closed(self):
        if os.geteuid() == 0:
            self.skipTest("running as root: permission bits would not block reads")
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            r2 = fx["wt"] / "docs" / "reports" / "r2.md"
            r2.chmod(0o000)
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 1)
            self.assertIn("unreadable document", result.stderr)

    def test_unreadable_subtree_fails_closed_while_sibling_repo_yields_files(self):
        # The fail-open case: an unreadable directory inside the scanned repo
        # used to be skipped silently, and the hub repo's documents still made
        # the run print OK and exit 0.
        if os.geteuid() == 0:
            self.skipTest("running as root: permission bits would not block reads")
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            broken = fx["wt"] / "docs" / "broken"
            broken.mkdir()
            (broken / "hidden.md").write_text(
                "# hidden\n\nNever listed.\n", encoding="utf-8")
            broken.chmod(0o000)
            try:
                result = self._run_cli(fx["wt"], "--workspace")
            finally:
                broken.chmod(0o755)
            self.assertEqual(
                result.returncode, 1,
                msg=f"unreadable subtree passed clean; stdout={result.stdout!r}")
            self.assertIn("unreadable subtree", result.stderr)
            self.assertIn("openxcvr-lib/docs/broken", result.stderr)
            # Exactly one finding: every readable document is still validated.
            self.assertEqual(result.stderr.strip().count("\n"), 0, result.stderr)

    def test_unreadable_excluded_directory_is_not_a_finding(self):
        if os.geteuid() == 0:
            self.skipTest("running as root: permission bits would not block reads")
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            excluded = fx["wt"] / "target" / "doc-build"
            excluded.mkdir(parents=True)
            (excluded / "generated.md").write_text("# gen\n", encoding="utf-8")
            excluded.chmod(0o000)
            try:
                result = self._run_cli(fx["wt"], "--workspace")
            finally:
                excluded.chmod(0o755)
            self.assertEqual(
                result.returncode, 0,
                msg=f"excluded tree is out of scope; stderr={result.stderr!r}")
            self.assertIn("OK: checked 7 documentation files (workspace)", result.stdout)

    def test_worktree_outside_workspace_root_validates_its_own_docs(self):
        # A worktree placed outside the workspace root must still be classified
        # as the owning repo: current-doc checks and cross-repo links apply.
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d), worktree_outside=True)
            wt = fx["wt"]
            self.assertFalse(str(wt).startswith(str(fx["ws"]) + os.sep))
            (wt / "docs" / "reports" / "wave-only.md").write_text(
                "# wave\n\nWorktree-only doc. See [r1](r1.md).\n", encoding="utf-8")
            readme = wt / "docs" / "reports" / "README.md"
            readme.write_text(
                readme.read_text(encoding="utf-8") + "- [wave](wave-only.md)\n",
                encoding="utf-8")
            result = self._run_cli(wt, "--workspace")
            self.assertEqual(
                result.returncode, 0,
                msg=f"stdout={result.stdout!r} stderr={result.stderr!r}")
            self.assertIn("OK: checked 8 documentation files (workspace)", result.stdout)

    def test_worktree_outside_workspace_root_keeps_current_doc_checks(self):
        # An orphan in the outside worktree is only visible if its docs are
        # classified as current docs of the owning repo.
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d), worktree_outside=True)
            (fx["wt"] / "docs" / "reports" / "orphan.md").write_text(
                "# orphan\n\nNot linked from anywhere.\n", encoding="utf-8")
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 1)
            self.assertIn("orphan", result.stderr)
            self.assertIn("openxcvr-lib/docs/reports/orphan.md", result.stderr)

    def test_reference_scope_seam_validates_from_worktree(self):
        # Extension seam for the wave1-docs reference tree: a manifest-driven
        # scope validates docs/reference from any checkout, worktree included.
        # The live registration is deferred until that tree exists.
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            ref = fx["wt"] / "docs" / "reference"
            ref.mkdir()
            (ref / "README.md").write_text(
                "# Reference\n\nGenerated reference index.\n\nSee [page](page.md).\n",
                encoding="utf-8")
            (ref / "page.md").write_text(
                "# Page\n\nA generated reference page.\n", encoding="utf-8")
            result = self._run_cli(fx["wt"], "--scope", "reference")
            self.assertEqual(
                result.returncode, 0,
                msg=f"stdout={result.stdout!r} stderr={result.stderr!r}")
            # The scope selects repos, so the whole worktree lib repo is
            # scanned (README + 4 receipts + 2 reference docs), with the
            # reference entry point and current-doc prefixes in force.
            self.assertIn("OK: checked 7 documentation files (reference)", result.stdout)

    def test_reference_scope_broken_link_fails(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            ref = fx["wt"] / "docs" / "reference"
            ref.mkdir()
            (ref / "README.md").write_text(
                "# Reference\n\nGenerated reference index.\n\nSee [page](page.md).\n",
                encoding="utf-8")
            (ref / "page.md").write_text(
                "# Page\n\nA generated reference page.\n\n[broken](missing.md)\n",
                encoding="utf-8")
            result = self._run_cli(fx["wt"], "--scope", "reference")
            self.assertEqual(result.returncode, 1)
            self.assertIn("broken Markdown link", result.stderr)

    def test_unknown_scope_is_rejected(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            result = self._run_cli(fx["wt"], "--scope", "nosuch")
            self.assertEqual(result.returncode, 1)
            self.assertIn("unknown scope", result.stderr)

    def test_scope_selector_is_exclusive(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            result = self._run_cli(fx["wt"], "--project", "--scope", "reference")
            self.assertEqual(result.returncode, 2)
            self.assertIn("exactly one", result.stderr)

    def test_cli_dangling_doc_symlinks_fail_closed_with_readable_peer(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            receipts = fx["wt"] / "docs" / "reports"
            (receipts / "dangling.md").symlink_to(receipts / "nonexistent.md")
            (receipts / "dangling.rst").symlink_to(receipts / "nonexistent.rst")
            (receipts / "dangling.3").symlink_to(receipts / "nonexistent.3")
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 1)
            self.assertIn("openxcvr-lib/docs/reports/dangling.md", result.stderr)
            self.assertIn("openxcvr-lib/docs/reports/dangling.rst", result.stderr)
            self.assertIn("openxcvr-lib/docs/reports/dangling.3", result.stderr)
            self.assertIn("unreadable document", result.stderr)

    def test_cli_healthy_doc_symlink_succeeds(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            (fx["wt"] / "docs" / "reports" / "sym.md").symlink_to(
                fx["wt"] / "docs" / "reports" / "r1.md")
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 0, msg=result.stderr)
            self.assertIn("OK: checked 8 documentation files (workspace)", result.stdout)

    def test_cli_symlink_loop_fails_closed(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            loop = fx["wt"] / "docs" / "reports" / "loop.md"
            loop.symlink_to(loop)
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 1)
            self.assertIn("openxcvr-lib/docs/reports/loop.md", result.stderr)
            self.assertIn("unreadable", result.stderr)

    def test_cli_directory_symlink_not_recursed(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            outside = Path(d) / "outside"
            outside.mkdir()
            (outside / "outside.md").write_text("# outside\n", encoding="utf-8")
            (fx["wt"] / "docs" / "reports" / "symdir").symlink_to(outside)
            result = self._run_cli(fx["wt"], "--workspace")
            self.assertEqual(result.returncode, 0, msg=result.stderr)
            self.assertIn("OK: checked 7 documentation files (workspace)", result.stdout)

    def test_inventory_withheld_on_missing_repo_root_preserves_existing(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            inv.write_bytes(b"# KNOWN GOOD INVENTORY\n")
            manifest = (fx["wt"] / "docs" / "fixture-manifest.toml").read_text(encoding="utf-8")
            manifest = manifest.replace(
                'repos = ["openxcvr", "openxcvr-lib"]',
                'repos = ["openxcvr", "openxcvr-lib", "sample-package"]')
            (fx["wt"] / "docs" / "fixture-manifest.toml").write_text(manifest, encoding="utf-8")
            (fx["lib"] / "docs" / "fixture-manifest.toml").write_text(manifest, encoding="utf-8")

            result = self._run_cli(fx["wt"], "--workspace", "--write-inventory")
            self.assertEqual(result.returncode, 1)
            self.assertEqual(inv.read_bytes(), b"# KNOWN GOOD INVENTORY\n")
            self.assertIn("refusing to refresh", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("missing repo root: sample-package", result.stderr)

    def test_inventory_withheld_on_missing_repo_root_does_not_create_absent(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            self.assertFalse(inv.exists())
            manifest = (fx["wt"] / "docs" / "fixture-manifest.toml").read_text(encoding="utf-8")
            manifest = manifest.replace(
                'repos = ["openxcvr", "openxcvr-lib"]',
                'repos = ["openxcvr", "openxcvr-lib", "sample-package"]')
            (fx["wt"] / "docs" / "fixture-manifest.toml").write_text(manifest, encoding="utf-8")
            (fx["lib"] / "docs" / "fixture-manifest.toml").write_text(manifest, encoding="utf-8")

            result = self._run_cli(fx["wt"], "--workspace", "--write-inventory")
            self.assertEqual(result.returncode, 1)
            self.assertFalse(inv.exists())
            self.assertIn("refusing to refresh", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("missing repo root: sample-package", result.stderr)

    def test_inventory_withheld_on_unreadable_subtree_preserves_existing(self):
        if os.geteuid() == 0:
            self.skipTest("running as root: permission bits would not block reads")
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            inv.write_bytes(b"# KNOWN GOOD INVENTORY\n")
            broken = fx["wt"] / "docs" / "broken"
            broken.mkdir()
            (broken / "hidden.md").write_text("# hidden\n", encoding="utf-8")
            broken.chmod(0o000)
            try:
                result = self._run_cli(fx["wt"], "--workspace", "--write-inventory")
            finally:
                broken.chmod(0o755)
            self.assertEqual(result.returncode, 1)
            self.assertEqual(inv.read_bytes(), b"# KNOWN GOOD INVENTORY\n")
            self.assertIn("refusing to refresh", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("unreadable subtree", result.stderr)

    def test_inventory_withheld_on_unreadable_subtree_does_not_create_absent(self):
        if os.geteuid() == 0:
            self.skipTest("running as root: permission bits would not block reads")
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            self.assertFalse(inv.exists())
            broken = fx["wt"] / "docs" / "broken"
            broken.mkdir()
            (broken / "hidden.md").write_text("# hidden\n", encoding="utf-8")
            broken.chmod(0o000)
            try:
                result = self._run_cli(fx["wt"], "--workspace", "--write-inventory")
            finally:
                broken.chmod(0o755)
            self.assertEqual(result.returncode, 1)
            self.assertFalse(inv.exists())
            self.assertIn("refusing to refresh", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("unreadable subtree", result.stderr)

    def test_inventory_withheld_on_unreadable_document_preserves_existing(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            inv.write_bytes(b"# KNOWN GOOD INVENTORY\n")
            (fx["wt"] / "docs" / "reports" / "dangling.md").symlink_to(
                fx["wt"] / "docs" / "reports" / "missing.md")
            result = self._run_cli(fx["wt"], "--workspace", "--write-inventory")
            self.assertEqual(result.returncode, 1)
            self.assertEqual(inv.read_bytes(), b"# KNOWN GOOD INVENTORY\n")
            self.assertIn("refusing to refresh", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("unreadable document", result.stderr)

    def test_inventory_withheld_on_unreadable_document_does_not_create_absent(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            self.assertFalse(inv.exists())
            (fx["wt"] / "docs" / "reports" / "dangling.md").symlink_to(
                fx["wt"] / "docs" / "reports" / "missing.md")
            result = self._run_cli(fx["wt"], "--workspace", "--write-inventory")
            self.assertEqual(result.returncode, 1)
            self.assertFalse(inv.exists())
            self.assertIn("refusing to refresh", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("unreadable document", result.stderr)

    def test_inventory_withheld_on_invalid_utf8_preserves_existing(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            inv.write_bytes(b"# KNOWN GOOD INVENTORY\n")
            (fx["wt"] / "docs" / "reports" / "r2.md").write_bytes(b"\xff\xfe\x00\x01\x80\xbf")
            result = self._run_cli(fx["wt"], "--workspace", "--write-inventory")
            self.assertEqual(result.returncode, 1)
            self.assertEqual(inv.read_bytes(), b"# KNOWN GOOD INVENTORY\n")
            self.assertIn("refusing to refresh", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("unreadable document", result.stderr)

    def test_inventory_withheld_on_invalid_utf8_does_not_create_absent(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            self.assertFalse(inv.exists())
            (fx["wt"] / "docs" / "reports" / "r2.md").write_bytes(b"\xff\xfe\x00\x01\x80\xbf")
            result = self._run_cli(fx["wt"], "--workspace", "--write-inventory")
            self.assertEqual(result.returncode, 1)
            self.assertFalse(inv.exists())
            self.assertIn("refusing to refresh", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("unreadable document", result.stderr)

    def test_inventory_withheld_on_empty_scope_preserves_existing(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            inv.write_bytes(b"# KNOWN GOOD INVENTORY\n")
            manifest = (fx["wt"] / "docs" / "fixture-manifest.toml").read_text(encoding="utf-8")
            empty_scope = '\n[scopes.empty]\nrepos = ["openxcvr-lib"]\nentry_points = []\ncurrent_doc_prefixes = []\n'
            manifest += empty_scope
            (fx["wt"] / "docs" / "fixture-manifest.toml").write_text(manifest, encoding="utf-8")
            (fx["lib"] / "docs" / "fixture-manifest.toml").write_text(manifest, encoding="utf-8")

            # Remove all markdown files from lib worktree
            for f in list((fx["wt"] / "docs").rglob("*.md")) + [fx["wt"] / "README.md"]:
                f.unlink()

            result = self._run_cli(fx["wt"], "--scope", "empty", "--write-inventory")
            self.assertEqual(result.returncode, 1)
            self.assertEqual(inv.read_bytes(), b"# KNOWN GOOD INVENTORY\n")
            self.assertIn("refusing to refresh", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("no documentation files found in scope", result.stderr)

    def test_inventory_withheld_on_empty_scope_does_not_create_absent(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            self.assertFalse(inv.exists())
            manifest = (fx["wt"] / "docs" / "fixture-manifest.toml").read_text(encoding="utf-8")
            empty_scope = '\n[scopes.empty]\nrepos = ["openxcvr-lib"]\nentry_points = []\ncurrent_doc_prefixes = []\n'
            manifest += empty_scope
            (fx["wt"] / "docs" / "fixture-manifest.toml").write_text(manifest, encoding="utf-8")
            (fx["lib"] / "docs" / "fixture-manifest.toml").write_text(manifest, encoding="utf-8")

            # Remove all markdown files from lib worktree
            for f in list((fx["wt"] / "docs").rglob("*.md")) + [fx["wt"] / "README.md"]:
                f.unlink()

            result = self._run_cli(fx["wt"], "--scope", "empty", "--write-inventory")
            self.assertEqual(result.returncode, 1)
            self.assertFalse(inv.exists())
            self.assertIn("refusing to refresh", result.stderr)
            self.assertIn("inputs are incomplete", result.stderr)
            self.assertIn("no documentation files found in scope", result.stderr)

    def test_inventory_refreshed_on_complete_scan_with_broken_link_and_exits_1(self):
        with tempfile.TemporaryDirectory() as d:
            fx = self._build_fixture(Path(d))
            inv = fx["hub"] / "docs" / "inventory.md"
            r1 = fx["wt"] / "docs" / "reports" / "r1.md"
            r1.write_text(r1.read_text(encoding="utf-8") + "\n[broken](missing.md)\n", encoding="utf-8")

            result = self._run_cli(fx["wt"], "--workspace", "--write-inventory")
            self.assertEqual(result.returncode, 1)
            self.assertTrue(inv.exists())
            text = inv.read_text(encoding="utf-8")
            self.assertIn("# Documentation Inventory", text)
            self.assertIn("- broken links: 1", text)
            self.assertIn("broken Markdown link", result.stderr)
            self.assertNotIn("refusing to refresh", result.stderr)


if __name__ == "__main__":
    unittest.main()
