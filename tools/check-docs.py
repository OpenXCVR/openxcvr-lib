#!/usr/bin/env python3
"""Validate the OpenXCVR workspace documentation.

Single source of truth for entry points, nav budgets, canonical/superseded
names, and persona routes is the supplied manifest. Edit the manifest, not the
constants here.

Checks:
  - entrypoints   : required nav files exist for the active scope
  - links         : local Markdown/RST links resolve
  - name drift    : superseded names only inside historical fences   (C3)
  - nav size      : entry/nav docs within line budget                (C1)
  - self-location : entry docs answer what/who/where-next up front    (C2)
  - reachability  : every current doc reachable from an entry point    (C4)
  - anchors       : #section links resolve to a real heading           (C4)
  - personas      : each persona's expected next-hops are reachable    (C4)

Scopes:
  - --project      validates the unified OpenXCVR project docs in ../openxcvr
  - --workspace    validates project docs plus local sibling checkouts
  - --all          backwards-compatible alias for --workspace
  - --scope NAME   validates the named [scopes.NAME] manifest section

Runs from a linked git worktree validate THIS worktree's copy of the owning
repository while resolving sibling repos in the owning workspace (located via
git metadata), never under .worktrees. Missing or unreadable repo roots,
unreadable in-scope subtrees and documents fail closed: an input the validator
cannot read is inconclusive, never clean. Directories excluded by name (see
EXCLUDE_PARTS) are pruned deliberately and are not inputs.

Optional [mounts].links declares workspace-logical path/source pairs: documents
at or under a source are validated at their linked path, and logical targets
under that path resolve back to the source. Missing sources fail closed and are
not remapped. Each logical document is checked once, preferring its source copy;
directory symlinks are still never traversed. Without mounts, positioning and
traversal are unchanged.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
import tomllib
from dataclasses import dataclass
from pathlib import Path
from urllib.parse import unquote


TOOL_REPO_ROOT = Path(__file__).resolve().parents[1]
CHECKOUT_ROOT = TOOL_REPO_ROOT


def resolve_main_repo_root(checkout_root: Path) -> Path | None:
    """Owning repository root of a checkout, from authoritative git metadata.

    A linked worktree's metadata names the main repository it belongs to; a
    main checkout is its own owning repository. Returns None when git cannot
    answer (no repository, no git binary): callers must fail closed rather
    than guess a workspace layout.
    """
    try:
        proc = subprocess.run(
            ["git", "-C", str(checkout_root), "rev-parse", "--git-common-dir"],
            capture_output=True, text=True, timeout=60)
    except (OSError, subprocess.TimeoutExpired):
        return None
    if proc.returncode != 0:
        return None
    common = Path(proc.stdout.strip())
    if not common.is_absolute():
        common = checkout_root / common
    main_root = common.resolve().parent
    if not (main_root / ".git").exists():
        # Layouts this tool does not model (e.g. a submodule's
        # .git/modules/<name> admin dir): refuse instead of deriving a
        # wrong workspace root.
        return None
    return main_root


def _bootstrap_workspace() -> None:
    """Resolve ROOT and REPO_BASES from this checkout's git metadata.

    Running from a linked worktree must validate THIS worktree's copy of the
    owning repository while resolving every sibling repo in the actual common
    workspace (the parent of the main repository) — never under .worktrees.
    Fails closed when the layout cannot be established.
    """
    global ROOT, OWNING_REPO, REPO_BASES
    main_root = resolve_main_repo_root(CHECKOUT_ROOT)
    if main_root is None:
        sys.exit(
            f"documentation checker: cannot resolve the owning repository of {CHECKOUT_ROOT} "
            "from git metadata; refusing to guess a workspace layout")
    owning = main_root.name
    if owning not in ALL_COMPONENT_REPOS:
        sys.exit(
            f"documentation checker: owning repository '{owning}' is not a workspace "
            f"component (expected one of: {', '.join(ALL_COMPONENT_REPOS)})")
    OWNING_REPO = owning
    ROOT = main_root.parent
    REPO_BASES = {owning: CHECKOUT_ROOT}


DEFAULT_REPOS = ()
ALL_COMPONENT_REPOS = ()
DOC_SUFFIXES = {".md", ".rst", ".3"}
EXCLUDE_PARTS = {
    ".git",
    ".kunit-uml",
    ".mypy_cache",
    ".parsed",
    ".ruff_cache",
    ".worktrees",
    "__pycache__",
    "node_modules",
    "target",
}
ARCHIVE_PARTS = set()
# A single conventional documentation manifest can be discovered locally.
# Nonstandard layouts and ambiguous directories require --manifest.
MANIFEST_PATH = None

# Provisional values; _bootstrap_workspace() sets the real ones and exits
# closed when git metadata cannot establish the layout.
ROOT: Path = CHECKOUT_ROOT.parent
OWNING_REPO: str = ""
REPO_BASES: dict[str, Path] = {}

_REQUIRED_KEYS = [
    ("entry_points", "required"),
    ("current_doc_prefixes", "prefixes"),
    ("canonical_names", "superseded"),
    ("nav_budgets", "default"),
]


def load_manifest(path: Path) -> dict:
    try:
        with path.open("rb") as fh:
            data = tomllib.load(fh)
    except (OSError, ValueError) as error:
        sys.exit(f"--manifest: {error}")
    for section, key in _REQUIRED_KEYS:
        if section not in data:
            sys.exit(f"--manifest: missing required section [{section}]")
        if key not in data[section]:
            sys.exit(f"--manifest: missing required key [{section}].{key}")
    if "mounts" in data:
        mounts = data["mounts"]
        if not isinstance(mounts, dict) or set(mounts) != {"links"}:
            sys.exit("--manifest: [mounts] must contain only links")
        if not isinstance(mounts["links"], list):
            sys.exit("--manifest: [mounts].links must be a list of tables")
        seen = {"path": set(), "source": set()}
        for link in mounts["links"]:
            if not isinstance(link, dict) or set(link) != {"path", "source"}:
                sys.exit("--manifest: each mount must contain exactly path and source")
            for key in ("path", "source"):
                value = link[key]
                if not isinstance(value, str):
                    sys.exit(f"--manifest: mount {key} must be a string")
                rel = Path(value)
                if rel.is_absolute() or len(rel.parts) < 2 or ".." in rel.parts:
                    sys.exit(f"--manifest: mount {key} must be workspace-logical (repo/path): {value}")
                if rel in seen[key]:
                    sys.exit(f"--manifest: duplicate mount {key}: {value}")
                nested = [other for other in seen[key] if rel in other.parents or other in rel.parents]
                if nested:
                    sys.exit(f"--manifest: mount {key} {value} is nested with {nested[0]}")
                seen[key].add(rel)
    return data


MANIFEST = {}
ACTIVE_SCOPE = "workspace"
ACTIVE_REPOS = ()
ACTIVE_PERSONAS: dict | None = None
ENTRY_POINTS: list[Path] = []
CURRENT_DOC_PREFIXES: tuple[Path, ...] = ()
STALE_TOKENS = ()
NAV_BUDGET_DEFAULT = 140
NAV_BUDGET_OVERRIDES = {}
REACH_EXEMPT_PREFIXES: tuple[Path, ...] = ()
SNAPSHOT_PREFIXES = ()


def initialize(path: Path) -> None:
    global MANIFEST_PATH, MANIFEST, DEFAULT_REPOS, ALL_COMPONENT_REPOS, ARCHIVE_PARTS
    global STALE_TOKENS, NAV_BUDGET_DEFAULT, NAV_BUDGET_OVERRIDES, SNAPSHOT_PREFIXES
    MANIFEST_PATH = path
    MANIFEST = load_manifest(path)
    DEFAULT_REPOS = tuple(MANIFEST["scopes"]["workspace"]["repos"])
    ALL_COMPONENT_REPOS = tuple(MANIFEST.get("workspace", {}).get("repos", DEFAULT_REPOS))
    ARCHIVE_PARTS = set(MANIFEST.get("workspace", {}).get("archive_parts", []))
    STALE_TOKENS = tuple(MANIFEST["canonical_names"]["superseded"])
    NAV_BUDGET_DEFAULT = MANIFEST["nav_budgets"]["default"]
    NAV_BUDGET_OVERRIDES = dict(MANIFEST["nav_budgets"].get("overrides", {}))
    SNAPSHOT_PREFIXES = tuple(Path(p) for p in MANIFEST.get("snapshots", {}).get("prefixes", []))
    _bootstrap_workspace()
    configure_scope("workspace")


def configure_scope(scope: str) -> None:
    """Load scope-specific manifest settings into the validator globals."""
    global ACTIVE_SCOPE, ACTIVE_REPOS, ACTIVE_PERSONAS
    global ENTRY_POINTS, CURRENT_DOC_PREFIXES, REACH_EXEMPT_PREFIXES

    scopes = MANIFEST.get("scopes", {})
    if scope not in scopes:
        sys.exit(
            f"--manifest: unknown scope '{scope}' "
            f"(defined: {', '.join(sorted(scopes)) or 'none'})")
    cfg = scopes[scope]
    ACTIVE_SCOPE = scope
    ACTIVE_REPOS = tuple(cfg.get("repos", DEFAULT_REPOS))
    ACTIVE_PERSONAS = cfg.get("personas")
    ENTRY_POINTS = [Path(p) for p in cfg.get(
        "entry_points", MANIFEST["entry_points"]["required"])]
    CURRENT_DOC_PREFIXES = tuple(Path(p) for p in cfg.get(
        "current_doc_prefixes", MANIFEST["current_doc_prefixes"]["prefixes"]))
    REACH_EXEMPT_PREFIXES = tuple(Path(p) for p in cfg.get(
        "reachability_exempt_prefixes",
        MANIFEST.get("reachability", {}).get("exempt_prefixes", [])))



# Markdown historical fence markers (invisible HTML comments).
HIST_BEGIN = "<!-- historical: begin -->"
HIST_END = "<!-- historical: end -->"
# RST historical fence markers.
RST_HIST_BEGIN = ".. historical::"
RST_HIST_END = ".. end-historical::"


@dataclass(frozen=True)
class Problem:
    path: Path
    line: int
    message: str

    def render(self) -> str:
        return f"{_logical(self.path)}:{self.line}: {self.message}"


MD_LINK_RE = re.compile(r"(?<!!)\[[^\]]+\]\(([^)]+)\)")
# Reference-style link definitions: [label]: url
MD_REF_DEF_RE = re.compile(r"^\[([^\]]+)\]:\s*(\S+)", re.MULTILINE)
# Reference-style link usages: [text][label] or [text][] (shortcut)
MD_REF_USE_RE = re.compile(r"(?<!!)\[([^\]]+)\]\[([^\]]*)\]")
RST_DOC_RE = re.compile(r":doc:`([^`]+)`")
RST_REF_RE = re.compile(r":ref:`([^`]+)`")
HEADING_RE = re.compile(r"^#{1,6}\s+(.*)$")
RST_TITLE_RE = re.compile(r"^[=\-~^\"#*+]{3,}\s*$")


def repo_root(repo: str) -> Path:
    """Physical checkout for a workspace repo name.

    The repo under validation (the owning repo) resolves to this checkout —
    the linked worktree when run from one, the main checkout otherwise; every
    other repo resolves to its sibling checkout under the workspace root.
    """
    return REPO_BASES.get(repo, ROOT / repo)


def _repo_bases() -> dict[str, Path]:
    bases = {repo: ROOT / repo for repo in ALL_COMPONENT_REPOS}
    bases.update(REPO_BASES)
    return bases


def _mounts():
    """Declared positions and physical sources (never recursively remapped)."""
    bases = _repo_bases()
    for link in MANIFEST.get("mounts", {}).get("links", []):
        position, source = Path(link["path"]), Path(link["source"])
        base = bases.get(source.parts[0])
        physical = base.joinpath(*source.parts[1:]) if base is not None else None
        yield position, source, physical


def _available_mounts():
    return [(position, physical) for position, _source, physical in _mounts()
            if physical is not None and physical.exists()]


def _mounted_path(rel: Path) -> Path | None:
    """Physical source for a logical mount position, or None if unavailable."""
    for position, physical in _available_mounts():
        if rel.is_relative_to(position):
            return physical / rel.relative_to(position)
    return None


def _locate(path: Path) -> tuple[str, Path] | None:
    """Logical position of a physical path: (repo name, repo-relative path).

    None when the path is outside every mapped checkout; callers fall back to
    ROOT-relative or physical handling, preserving pre-worktree behaviour.
    """
    for position, source in _available_mounts():
        if path.is_relative_to(source):
            logical = position / path.relative_to(source)
            return logical.parts[0], Path(*logical.parts[1:])
    for repo, base in sorted(_repo_bases().items(),
                             key=lambda kv: len(kv[1].parts), reverse=True):
        try:
            rel = path.relative_to(base)
        except ValueError:
            continue
        return repo, rel
    return None


def _logical(path: Path) -> Path:
    """Workspace-logical path (repo name first) for classification/display."""
    loc = _locate(path)
    if loc is not None:
        return Path(loc[0], *loc[1].parts)
    try:
        return path.relative_to(ROOT)
    except ValueError:
        return path


def workspace_path(rel: Path) -> Path:
    """Physical path for a workspace-logical path (entry points, persona starts)."""
    mounted = _mounted_path(rel)
    if mounted is not None:
        return mounted
    parts = rel.parts
    if parts and parts[0] in ALL_COMPONENT_REPOS:
        return repo_root(parts[0]).joinpath(*parts[1:])
    return ROOT / rel


def _resolve_local_target(path: Path, target: str) -> Path:
    """Resolve a local relative link target against the workspace view.

    The source document's position is logical (repo + repo-relative path), so
    a target that climbs out of its repo lands in the sibling checkout of the
    workspace — a plain sibling for other repos, the selected worktree for
    the repo under validation. Targets that escape the workspace view fall
    back to plain filesystem resolution (pre-worktree behaviour).
    """
    loc = _locate(path)
    if loc is not None:
        repo, rel = loc
        logical_dir = Path(repo, *rel.parts).parent
        joined = Path(os.path.normpath(str(logical_dir / target)))
        if joined.parts and joined.parts[0] in _repo_bases():
            mounted = _mounted_path(joined)
            if mounted is not None:
                return mounted.resolve()
            return repo_root(joined.parts[0]).joinpath(*joined.parts[1:]).resolve()
    return (path.parent / target).resolve()


def ignored(path: Path) -> bool:
    rel = _logical(path)
    return (any(part in EXCLUDE_PARTS for part in rel.parts)
            or any(rel == pre or pre in rel.parents for pre in SNAPSHOT_PREFIXES))


def _active_bases() -> tuple[Path, ...]:
    return tuple(repo_root(repo) for repo in ACTIVE_REPOS)


def _unreadable(path: Path, what: str, exc: OSError) -> Problem:
    detail = getattr(exc, "strerror", None) or exc.__class__.__name__
    return Problem(
        path, 0,
        f"unreadable {what} ({detail}): validation is inconclusive")


def _scan_doc_tree(base: Path, scandir) -> tuple[list[Path], list[Problem]]:
    """Error-aware recursive scan of one checkout for documentation files.

    Returns the documents found and a finding for every in-scope directory (or
    entry) whose contents cannot be listed. A recursive glob is deliberately not
    used: pathlib silently skips directories it cannot read, which would drop
    their documents with no finding. One unreadable directory is a finding and
    never removes the rest of the repository (or the other repositories) from
    the scan.

    Directories named in EXCLUDE_PARTS are pruned explicitly: their contents are
    out of scope, so an unreadable excluded directory is not a finding. Pruning
    is by directory name inside the scanned checkout, so the selected worktree
    is never excluded by an ancestor `.worktrees` component of its own path.
    """
    files: list[Path] = []
    problems: list[Problem] = []
    stack = [base]
    while stack:
        directory = stack.pop()
        try:
            entries = scandir(directory)
            try:
                children = list(entries)
            finally:
                close = getattr(entries, "close", None)
                if close is not None:
                    close()
        except OSError as exc:
            problems.append(_unreadable(directory, "subtree", exc))
            continue
        for entry in children:
            path = directory / entry.name
            try:
                # Symlinked directories are not followed (matching the previous
                # recursive glob); symlinked documents are still collected.
                if entry.is_dir(follow_symlinks=False):
                    if entry.name not in EXCLUDE_PARTS and not ignored(path):
                        stack.append(path)
                elif (entry.is_symlink() or entry.is_file()) and path.suffix in DOC_SUFFIXES and not ignored(path):
                    files.append(path)
            except OSError as exc:
                problems.append(_unreadable(path, "entry", exc))
    problems.sort(key=lambda problem: str(problem.path))
    return files, problems


def doc_files(scandir=os.scandir) -> tuple[list[Path], list[Problem]]:
    """Documents in scope, plus findings for unreadable in-scope subtrees.

    `scandir` is injectable so traversal failures can be exercised
    deterministically without depending on permission bits.
    """
    files: list[Path] = []
    problems: list[Problem] = []
    for base in _active_bases():
        if not base.exists():
            continue  # reported as a missing repo root by check_repo_roots()
        found, failures = _scan_doc_tree(base, scandir)
        files.extend(found)
        problems.extend(failures)
    for position, source, physical in _mounts():
        if physical is None or not physical.exists():
            problems.append(Problem(
                physical if physical is not None else ROOT / source, 0,
                f"missing mount source: {source} (mount {position}): validation is inconclusive"))
        elif position.parts[0] in ACTIVE_REPOS and source.parts[0] not in ACTIVE_REPOS:
            # The material belongs to an active repository but is kept in an
            # inactive one: scan its source, which the walk above did not reach.
            if physical.is_dir():
                found, failures = _scan_doc_tree(physical, scandir)
                files.extend(found)
                problems.extend(failures)
            elif physical.suffix in DOC_SUFFIXES:
                files.append(physical)
    sources = [source for _position, source in _available_mounts()]
    by_position: dict[Path, Path] = {}
    # Source paths sort first, so a library-side file symlink cannot duplicate
    # its source document or replace it as the graph's authoritative input.
    for path in sorted(files, key=lambda p: (
            not any(p.is_relative_to(source) for source in sources), str(p))):
        by_position.setdefault(_logical(path), path)
    return sorted(by_position.values()), problems


def is_archive(path: Path) -> bool:
    return any(part in ARCHIVE_PARTS for part in _logical(path).parts)


def is_current_doc(path: Path) -> bool:
    rel = _logical(path)
    if is_archive(path):
        return False
    return any(rel == prefix or prefix in rel.parents for prefix in CURRENT_DOC_PREFIXES)


def target_is_external(target: str) -> bool:
    return bool(
        re.match(r"^[a-zA-Z][a-zA-Z0-9+.-]*:", target)
        or target.startswith("//")
    )


def resolve_external_workspace_link(path: Path, target: str) -> Path | None:
    """Map GitHub OpenXCVR repo links to local sibling checkouts.

    Component repos often link to GitHub paths. During workspace validation,
    map known OpenXCVR repos back to local sibling checkouts when present.
    """
    if ACTIVE_SCOPE != "workspace":
        return None
    raw = target.strip().strip("<>").split("#", 1)[0]
    match = re.match(
        r"https://github\.com/([^/]+)/([^/#?]+)(?:/(?:blob|tree)/[^/]+/(.*))?/?$",
        raw,
        re.IGNORECASE,
    )
    if not match:
        return None
    org, repo, rest = match.groups()
    repo = repo.removesuffix(".git")
    if org.lower() != "openxcvr":
        return None
    repo = MANIFEST.get("workspace", {}).get("aliases", {}).get(repo, repo)
    if repo in ALL_COMPONENT_REPOS:
        local = repo_root(repo)
    else:
        return None
    rel = Path(rest) if rest else Path("README.md")
    logical = Path(repo) / rel
    if any(logical == Path(link["path"]) or Path(link["path"]) in logical.parents
           for link in MANIFEST.get("mounts", {}).get("links", [])):
        # Mounted material is linked into a checkout, never committed, so the
        # URL names nothing on GitHub: report it whether or not this checkout
        # holds the link. (No path under the null device exists.)
        return Path(os.devnull) / "unpublished-mount" / logical
    return (local / rel).resolve()


def normalize_target(target: str) -> str:
    target = target.strip()
    if " " in target and not target.startswith("<"):
        target = target.split()[0]
    target = target.strip("<>")
    target = target.split("#", 1)[0]
    return unquote(target)


def resolve_markdown_link(path: Path, target: str) -> Path | None:
    external = resolve_external_workspace_link(path, target)
    if external is not None:
        return external
    target = normalize_target(target)
    if not target or target.startswith("#") or target_is_external(target):
        return None
    return _resolve_local_target(path, target)


def resolve_rst_doc(path: Path, target: str) -> Path | None:
    if "<" in target and ">" in target:
        target = target.rsplit("<", 1)[1].rstrip(">")
    target = normalize_target(target)
    if not target or target.startswith("#") or target_is_external(target):
        return None
    candidate = _resolve_local_target(path, target)
    if candidate.suffix:
        return candidate
    return candidate.with_suffix(".rst")


def _ref_defs(text: str) -> dict[str, str]:
    """Return {lowercase_label: url} for all reference-style link definitions."""
    return {m.group(1).lower(): m.group(2) for m in MD_REF_DEF_RE.finditer(text)}


def _slugify(text: str) -> str:
    # Keep underscores to match GitHub's heading anchor algorithm.
    s = re.sub(r"[^a-z0-9\-_ ]", "", text.strip().lower())
    return s.replace(" ", "-")


def _doc_headings(path: Path) -> set[str]:
    """Return all valid anchor slugs for headings, including -1/-2 dedup variants."""
    out: set[str] = set()
    counts: dict[str, int] = {}
    try:
        lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
    except OSError:
        # Unreadable input is already a finding (check_unreadable); it must
        # not crash the run before findings are printed.
        return out
    for line in lines:
        m = HEADING_RE.match(line)
        if m:
            slug = _slugify(m.group(1))
            if slug in counts:
                counts[slug] += 1
                out.add(f"{slug}-{counts[slug]}")
            else:
                counts[slug] = 0
                out.add(slug)
    return out


def _outbound_local_links(path: Path):
    """Yield (resolved_path_or_None, fragment) for local links (inline + reference-style)."""
    try:
        text = path.read_text(encoding="utf-8", errors="replace")
    except OSError:
        # Unreadable input is already a finding (check_unreadable); traversal
        # must not crash before findings are printed.
        return
    defs = _ref_defs(text)
    for m in MD_LINK_RE.finditer(text):
        raw = m.group(1)
        frag = raw.split("#", 1)[1] if "#" in raw else ""
        yield resolve_markdown_link(path, raw), frag
    for m in MD_REF_USE_RE.finditer(text):
        label = (m.group(2) or m.group(1)).lower()
        raw = defs.get(label)
        if raw:
            frag = raw.split("#", 1)[1] if "#" in raw else ""
            yield resolve_markdown_link(path, raw), frag


def _reachable_from(starts: list[Path]) -> set[Path]:
    """DFS the local-link graph from the given start paths."""
    seen: set[Path] = set()
    queue = [s.resolve() for s in starts if s.exists()]
    for q in queue:
        seen.add(q)
    while queue:
        node = queue.pop()
        if node.suffix not in DOC_SUFFIXES or not node.exists():
            continue
        for resolved, _frag in _outbound_local_links(node):
            if resolved and resolved not in seen and resolved.exists():
                seen.add(resolved)
                queue.append(resolved)
    return seen


def check_links(files: list[Path]) -> list[Problem]:
    problems: list[Problem] = []
    for path in files:
        if is_archive(path):
            continue
        text = path.read_text(encoding="utf-8", errors="replace")
        defs = _ref_defs(text)
        for line_no, line in enumerate(text.splitlines(), 1):
            for match in MD_LINK_RE.finditer(line):
                resolved = resolve_markdown_link(path, match.group(1))
                if resolved and resolved.suffix.lower() == ".pdf":
                    continue
                if resolved and not resolved.exists():
                    problems.append(Problem(path, line_no, f"broken Markdown link: {match.group(1)}"))
            for match in MD_REF_USE_RE.finditer(line):
                label = (match.group(2) or match.group(1)).lower()
                raw = defs.get(label)
                if raw:
                    resolved = resolve_markdown_link(path, raw)
                    if resolved and resolved.suffix.lower() == ".pdf":
                        continue
                    if resolved and not resolved.exists():
                        problems.append(Problem(path, line_no, f"broken reference link [{label}]: {raw}"))
            # RST roles are executable links only in reStructuredText: the
            # documented format contract gives :doc:/:ref: meaning in RST
            # (the Sphinx build configures source_suffix = '.rst'), so
            # role-shaped text in ordinary Markdown (.md/.3) is inert prose,
            # never a link to resolve.
            if path.suffix == ".rst":
                for regex, kind in ((RST_DOC_RE, "RST doc link"), (RST_REF_RE, "RST ref")):
                    for match in regex.finditer(line):
                        resolved = resolve_rst_doc(path, match.group(1))
                        if resolved and not resolved.exists() and kind == "RST doc link":
                            problems.append(Problem(path, line_no, f"broken {kind}: {match.group(1)}"))
    return problems


def check_name_drift(files: list[Path]) -> list[Problem]:
    """Superseded names in current docs are flagged unless inside a historical fence.

    Markdown: <!-- historical: begin --> ... <!-- historical: end -->
    RST:      .. historical::  ... .. end-historical::

    Block-scoped, not word-on-line. Tokens on the RST directive line are still
    flagged. Unclosed Markdown fences are reported as an error.
    """
    problems: list[Problem] = []
    for path in files:
        if not is_current_doc(path):
            continue
        in_hist = False
        fence_open_line = 0
        raw_lines = path.read_text(encoding="utf-8", errors="replace").splitlines()
        for line_no, line in enumerate(raw_lines, 1):
            stripped = line.strip()

            # Markdown fence begin: enter fence; do NOT check tokens on this line.
            if stripped == HIST_BEGIN:
                in_hist = True
                fence_open_line = line_no
                continue

            # RST fence end.
            if stripped == RST_HIST_END:
                in_hist = False
                fence_open_line = 0
                continue

            # Markdown fence end.
            if stripped == HIST_END:
                in_hist = False
                fence_open_line = 0
                continue

            # RST fence begin: enter fence; fall through to token check
            # so that stale tokens ON the directive line are still flagged.
            if stripped.startswith(RST_HIST_BEGIN):
                in_hist = True
                fence_open_line = line_no

            if in_hist and not stripped.startswith(RST_HIST_BEGIN):
                continue

            low = line.lower()
            if any(tok in low for tok in STALE_TOKENS):
                problems.append(Problem(
                    path, line_no,
                    "superseded name in current doc (wrap in <!-- historical: begin/end --> if intentional)"))

        if in_hist:
            problems.append(Problem(
                path, fence_open_line,
                "unclosed historical fence: <!-- historical: begin --> has no matching <!-- historical: end -->"))
    return problems


def check_entrypoints() -> list[Problem]:
    problems: list[Problem] = []
    for rel in ENTRY_POINTS:
        if not workspace_path(rel).exists():
            problems.append(Problem(workspace_path(rel).parent, 0, f"missing entrypoint: {rel}"))
    return problems


def check_repo_roots() -> list[Problem]:
    """Fail closed on scope repos whose checkout is missing or unreadable:
    a silently skipped repo would pass with a subset of the workspace."""
    problems: list[Problem] = []
    for repo in ACTIVE_REPOS:
        base = repo_root(repo)
        if not base.exists():
            problems.append(Problem(base, 0, f"missing repo root: {repo} (expected at {base})"))
            continue
        try:
            next(base.iterdir())
        except StopIteration:
            continue  # exists and is listable; merely empty
        except OSError as exc:
            problems.append(Problem(base, 0, f"unreadable repo root: {repo} ({exc})"))
    return problems


def check_unreadable(files: list[Path]) -> list[Problem]:
    """Fail closed on documents that cannot be read: an unreadable input is
    inconclusive, never clean."""
    problems: list[Problem] = []
    for path in files:
        try:
            path.read_text(encoding="utf-8")
        except (OSError, UnicodeDecodeError) as exc:
            detail = getattr(exc, "strerror", None) or exc.__class__.__name__
            problems.append(Problem(
                path, 0, f"unreadable document ({detail}): validation is inconclusive"))
    return problems


def check_nav_size() -> list[Problem]:
    problems: list[Problem] = []
    for rel in ENTRY_POINTS:
        path = workspace_path(rel)
        if not path.exists():
            continue
        budget = NAV_BUDGET_OVERRIDES.get(str(rel), NAV_BUDGET_DEFAULT)
        try:
            n = len(path.read_text(encoding="utf-8", errors="replace").splitlines())
        except OSError:
            continue  # already reported by check_unreadable
        if n > budget:
            problems.append(Problem(path, 0, f"exceeds nav size budget: {n} > {budget} lines"))
    return problems


def check_self_location() -> list[Problem]:
    """Entry docs must answer what/who/where-next in the first 20 non-blank lines:
    a title, a prose purpose line, and at least one outbound local doc link
    (inline, reference-style, or RST :doc:`)."""
    problems: list[Problem] = []
    for rel in ENTRY_POINTS:
        path = workspace_path(rel)
        if not path.exists():
            continue
        try:
            full_text = path.read_text(encoding="utf-8", errors="replace")
        except OSError:
            continue  # already reported by check_unreadable
        lines = [ln for ln in full_text.splitlines() if ln.strip()][:20]
        blob = "\n".join(lines)
        defs = _ref_defs(full_text)
        is_rst = path.suffix == ".rst"
        has_title = any(ln.startswith("# ") for ln in lines) or (
            is_rst and any(RST_TITLE_RE.match(ln) for ln in lines))
        has_prose = any(
            ln.strip() and not ln.startswith(("#", "=", "-", "|", "```", ".. ", ":"))
            for ln in lines)
        has_inline_link = bool(MD_LINK_RE.search(blob))
        has_ref_link = any(
            (m.group(2) or m.group(1)).lower() in defs
            for m in MD_REF_USE_RE.finditer(blob))
        has_link = has_inline_link or has_ref_link or (":doc:`" in blob)
        missing = [n for n, ok in (("title", has_title), ("purpose", has_prose), ("next-hop", has_link)) if not ok]
        if missing:
            problems.append(Problem(
                path, 0, f"self-location: entry doc missing {', '.join(missing)} in first 20 lines"))
    return problems


def _reach_exempt(path: Path) -> bool:
    rel = _logical(path)
    return any(rel == pre or pre in rel.parents for pre in REACH_EXEMPT_PREFIXES)


def check_reachability(files: list[Path]) -> list[Problem]:
    current = {p.resolve() for p in files if is_current_doc(p) and not _reach_exempt(p)}
    seen = _reachable_from([workspace_path(e) for e in ENTRY_POINTS])
    orphans = sorted(current - seen)
    return [Problem(p, 0, "orphan: not reachable from any entry point") for p in orphans]


def check_anchors(files: list[Path]) -> list[Problem]:
    problems: list[Problem] = []
    heading_cache: dict[Path, set[str]] = {}
    for path in files:
        if is_archive(path):
            continue
        for resolved, frag in _outbound_local_links(path):
            if not resolved or not frag or not resolved.exists():
                continue
            # RST anchor resolution uses a different model; skip non-.md targets.
            if resolved.suffix.lower() != ".md":
                continue
            if resolved not in heading_cache:
                heading_cache[resolved] = _doc_headings(resolved)
            if _slugify(frag) not in heading_cache[resolved]:
                problems.append(Problem(path, 0, f"dangling anchor: {resolved.name}#{frag}"))
    return problems


def check_personas() -> list[Problem]:
    """For each persona, BFS from its start doc and require every expected
    next-hop target to be reachable along that persona's own path (C4)."""
    problems: list[Problem] = []
    routes = ACTIVE_PERSONAS if ACTIVE_PERSONAS is not None else MANIFEST.get("personas", {})
    for name, route in routes.items():
        start = workspace_path(Path(route["start"]))
        if not start.exists():
            problems.append(Problem(start, 0, f"persona '{name}': start doc missing"))
            continue
        reachable = _reachable_from([start])
        for target in route.get("expect", []):
            tgt = workspace_path(Path(target)).resolve()
            if tgt not in reachable:
                problems.append(Problem(
                    start, 0, f"persona '{name}': {target} not reachable from {route['start']}"))
    return problems


def _scope_flag() -> str:
    """The selector that reproduces this run, for the generated header."""
    if ACTIVE_SCOPE in ("project", "workspace"):
        return f"--{ACTIVE_SCOPE}"
    return f"--scope {ACTIVE_SCOPE}"


def inventory(files, name_probs, orphan_probs, anchor_probs, link_probs) -> str:
    scope_flag = _scope_flag()
    lines = [
        "# Documentation Inventory",
        "",
        f"Generated by `openxcvr-lib/tools/{Path(__file__).name} {scope_flag} --write-inventory`.",
        "Signal only — counts, entry points (single source), and health. The full",
        "file list is intentionally omitted; run the validator to enumerate docs.",
        "",
        "## Counts",
        "",
    ]
    for repo in ACTIVE_REPOS:
        base = repo_root(repo)
        count = sum(1 for path in files if path == base or base in path.parents)
        lines.append(f"- `{repo}`: {count} documentation files")
    lines += ["", f"## Entry points (single source: openxcvr-lib/{MANIFEST_PATH.relative_to(TOOL_REPO_ROOT) if MANIFEST_PATH.is_relative_to(TOOL_REPO_ROOT) else MANIFEST_PATH})", ""]
    lines += [f"- `{p}`" for p in (str(e) for e in ENTRY_POINTS)]
    lines += [
        "",
        "## Health summary",
        "",
        f"- superseded names in current docs: {len(name_probs)}",
        f"- broken links: {len(link_probs)}",
        f"- dangling anchors: {len(anchor_probs)}",
        f"- orphaned current docs: {len(orphan_probs)}",
        "",
    ]
    return "\n".join(lines)


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--all", action="store_true", help="alias for --workspace")
    parser.add_argument("--project", action="store_true", help="validate unified openxcvr project docs")
    parser.add_argument("--workspace", action="store_true", help="validate project docs plus local sibling checkouts")
    parser.add_argument("--scope", metavar="NAME",
                        help="validate the named [scopes.NAME] section of manifest")
    parser.add_argument("--write-inventory", action="store_true", help="refresh docs/inventory.md")
    parser.add_argument("--manifest", type=Path, help="Documentation configuration (default: the sole docs/*manifest.toml)")
    args = parser.parse_args(argv)
    manifests = sorted((TOOL_REPO_ROOT / "docs").glob("*manifest.toml"))
    if args.manifest is None and len(manifests) != 1:
        parser.error("--manifest is required when docs/ does not contain exactly one manifest")
    initialize(args.manifest or manifests[0])

    selected = [args.all, args.project, args.workspace, args.scope is not None]
    if sum(bool(v) for v in selected) != 1:
        parser.error("pass exactly one of --all, --project, --workspace, or --scope")
    configure_scope(args.scope if args.scope is not None
                    else ("project" if args.project else "workspace"))

    files, traversal_probs = doc_files()
    root_probs = check_repo_roots()
    read_probs = check_unreadable(files)
    if not files and not root_probs and not traversal_probs:
        root_probs = root_probs + [Problem(
            repo_root(ACTIVE_REPOS[0]), 0,
            "no documentation files found in scope: refusing to pass an empty validation")]
    inputs_complete = not bool(root_probs or traversal_probs or read_probs)
    # Unreadable inputs are already findings; do not also run content checks
    # that would crash on them.
    unreadable = {p.path for p in read_probs}
    checkable = [f for f in files if f not in unreadable]
    entry_probs = check_entrypoints()
    link_probs = check_links(checkable)
    name_probs = check_name_drift(checkable)
    size_probs = check_nav_size()
    selfloc_probs = check_self_location()
    orphan_probs = check_reachability(checkable)
    anchor_probs = check_anchors(checkable)
    persona_probs = check_personas()
    problems = (root_probs + traversal_probs + read_probs + entry_probs + link_probs
                + name_probs + size_probs + selfloc_probs + orphan_probs
                + anchor_probs + persona_probs)

    if args.write_inventory:
        inventory_rel = Path(
            MANIFEST.get("inventory", {}).get("path", "openxcvr/docs/inventory.md"))
        inventory_path = workspace_path(inventory_rel)
        if inputs_complete:
            inventory_path.write_text(
                inventory(files, name_probs, orphan_probs, anchor_probs, link_probs),
                encoding="utf-8")
        else:
            print(
                f"refusing to refresh {inventory_rel}: inputs are incomplete (validation is inconclusive)",
                file=sys.stderr)

    if problems:
        for problem in problems:
            print(problem.render(), file=sys.stderr)
        return 1

    print(f"OK: checked {len(files)} documentation files ({ACTIVE_SCOPE})")
    if args.write_inventory:
        print("OK: refreshed docs/inventory.md")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
