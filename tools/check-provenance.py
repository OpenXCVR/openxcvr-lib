#!/usr/bin/env python3
"""OpenXCVR Provenance and Donor Inventory Validator.

Closed-world reuse gate validator for openxcvr-lib.
Validates ledger against tests/fixtures/parity/donor_inventory.yaml:

- Closed-world inventory accounting: every kmod source the library reuses
  (donated_logic, source_only_comparison or generated_schema_semantics) has a
  ledger section, and every section maps to an inventory row.
- Inventory structure: stable unique row ids, the six Task-1 dispositions,
  donated_logic rows carry fixture ids AND an adapter id, every non-donated row
  carries an explicit rationale, kmod paths pin the donor pin, and the inventory is a
  partition (no source line claimed by two rows with different dispositions or
  owners, except an explicit depends_on dependency record).
- Authorship: section author and committer lists equal the retained observed
  history for the file in BOTH directions (authors and committers, automation
  identities and the GitHub web PR merge committer included), and every listed
  identity is attributed under Petabit Scale's recorded MIT grant (supplied attribution policy), or for the GitHub committer as platform merge automation that
  never authors; a `clear` verdict with any unattributed author or committer is
  refused.
- Fails closed on clear verdicts: a section must carry kmod_commits,
  pre_split_history, grant and lib_commits lines, and `clear` requires positive
  evidence that full history was checked (a `checked:` pre_split_history value);
  absent, "not checked", "unknown" or any other non-affirmative value with
  `clear` is refused, as is unreachable-plus-clear.
- Optional --require-landings: each cleared file's lib_path must exist at its
  own landing commit/range endpoint in the openxcvr-lib repository (located from
  this tool's checkout, never the caller's cwd), and a stopped file can never
  have landed.
- Optional --kmod-dir: re-derives observed authors and committers from a live
  donor repository clone, refuses clear verdicts for files that exist at split root
  the split root (pre-split history unreachable), and verifies that every kmod_commits
  endpoint resolves and contains the file. A pinned revision (the donor pin, the split root)
  or a cited endpoint that does not resolve in the clone is an error, never a
  vacuous pass.
"""

from __future__ import annotations

import argparse
import os
import re
import subprocess
import sys
from collections import defaultdict
from pathlib import Path
from typing import Any, Dict, List, Optional, Set, Tuple

try:
    import yaml
except ImportError:
    yaml = None


# Authorship facts are supplied data, never a built-in grant.
ATTRIBUTED_IDENTITIES: Dict[str, str] = {}
RETAINED_OBSERVED_AUTHORS: Dict[str, Set[str]] = {}
RETAINED_OBSERVED_COMMITTERS: Dict[str, Set[str]] = {}
KMOD_SPLIT_ROOT = ""
KMOD_DONOR_COMMIT = ""
KMOD_DONOR_COMMIT_SHA = ""


def load_authorship(path):
    """Load the retained identities and history; missing facts are inconclusive."""
    import json
    global ATTRIBUTED_IDENTITIES, RETAINED_OBSERVED_AUTHORS, RETAINED_OBSERVED_COMMITTERS
    global KMOD_SPLIT_ROOT, KMOD_DONOR_COMMIT, KMOD_DONOR_COMMIT_SHA
    try:
        data = json.loads(Path(path).read_text(encoding="utf-8"))
        identities = data["ATTRIBUTED_IDENTITIES"]
        authors = data["RETAINED_OBSERVED_AUTHORS"]
        committers = data["RETAINED_OBSERVED_COMMITTERS"]
        pins = [data[k] for k in ("KMOD_SPLIT_ROOT", "KMOD_DONOR_COMMIT", "KMOD_DONOR_COMMIT_SHA")]
        if not isinstance(identities, dict) or not identities or not all(
                isinstance(k, str) and isinstance(v, str) and v for k, v in identities.items()):
            raise ValueError("invalid attributed identities")
        for table in (authors, committers):
            if not isinstance(table, dict) or not table or not all(
                    isinstance(k, str) and isinstance(v, list) and v and
                    all(isinstance(i, str) and i for i in v) for k, v in table.items()):
                raise ValueError("invalid retained history")
        if not all(isinstance(pin, str) and re.fullmatch(r"[0-9a-f]{7,40}", pin) for pin in pins):
            raise ValueError("invalid history pins")
    except (OSError, ValueError, KeyError, TypeError) as error:
        raise ProvenanceValidationError(f"--authorship: {error}") from error
    ATTRIBUTED_IDENTITIES = identities
    RETAINED_OBSERVED_AUTHORS = {k: set(v) for k, v in authors.items()}
    RETAINED_OBSERVED_COMMITTERS = {k: set(v) for k, v in committers.items()}
    KMOD_SPLIT_ROOT, KMOD_DONOR_COMMIT, KMOD_DONOR_COMMIT_SHA = pins


class ProvenanceValidationError(Exception):
    """Raised when provenance or inventory validation fails."""


# The six Task-1 disposition categories, plus normalization.
VALID_DISPOSITIONS: Set[str] = {
    "donated_logic",
    "generated_schema_semantics",
    "kernel_only_infrastructure",
    "consumer_execution",
    "source_only_comparison",
    "explicit_non_donor",
}

# Dispositions whose kmod source the library reuses, so a ledger section is required.
SECTION_REQUIRED_DISPOSITIONS: Set[str] = {
    "donated_logic",
    "generated_schema_semantics",
    "source_only_comparison",
}

VALID_ADAPTERS: Set[str] = {
    "cmis_thermal_decide",
    "xcvr_page_class_of",
    "cmis_module_walk_to_ready",
    "xcvr_probe_lifecycle",
    "xcvr_sched_decisions",
    "xcvr_operation_plans",
    "xcvr_cache_decisions",
    "cmis_thermal_runtime",
    "cmis_snapshot_latches",
    "none",
}

VALID_OWNER_RE = re.compile(r"^(task\d+|kernel|consumer|daemon|none|kmod-generated-tables)$")

# Required per-section fields (the task prompt's exact field names).
REQUIRED_SECTION_FIELDS = (
    "kmod_path",
    "kmod_commits",
    "pre_split_history",
    "authors",
    "committers",
    "grant",
    "verdict",
    "lib_commits",
    "donor_commit",
    "extraction_symbols",
    "extraction_ranges",
    "dependent_tasks",
    "raw_evidence",
)

# Affirmative full-history-checked marker for a `clear` verdict.
PRE_SPLIT_CHECKED_PREFIX = "checked:"


def normalize_disposition(value: Any) -> str:
    return str(value).strip().lower().replace("-", "_").replace(" ", "_")


def parse_ranges(value: Any) -> List[Tuple[int, int]]:
    """Parse '193,320-328,354' into [(193,193),(320,328),(354,354)]."""
    out: List[Tuple[int, int]] = []
    for part in str(value).split(","):
        part = part.strip()
        if not part:
            continue
        if "-" in part:
            a, b = part.split("-", 1)
            try:
                lo, hi = int(a), int(b)
            except ValueError:
                raise ProvenanceValidationError(f"invalid range component '{part}'")
        else:
            try:
                lo = hi = int(part)
            except ValueError:
                raise ProvenanceValidationError(f"invalid range component '{part}'")
        if lo > hi or lo < 1:
            raise ProvenanceValidationError(f"invalid range component '{part}'")
        out.append((lo, hi))
    if not out:
        raise ProvenanceValidationError("empty source_range")
    return out


def ranges_overlap(a: List[Tuple[int, int]], b: List[Tuple[int, int]]) -> bool:
    return any(max(x1, y1) <= min(x2, y2) for x1, x2 in a for y1, y2 in b)


def is_kmod_path(path: str) -> bool:
    return path.startswith(("core/", "plugins/", "include/xcvr/", "include/trace/"))


def parse_provenance(path_or_text: str) -> Dict[str, Dict[str, Any]]:
    """Parse ledger into structured sections keyed by kmod_path."""
    if os.path.exists(path_or_text):
        with open(path_or_text, "r", encoding="utf-8") as f:
            text = f.read()
    else:
        text = path_or_text

    sections: Dict[str, Dict[str, Any]] = {}
    current_section: Optional[Dict[str, Any]] = None
    current_path: Optional[str] = None

    def close_current() -> None:
        nonlocal current_section, current_path
        if current_path and current_section:
            sections[current_path] = current_section
        current_section = None
        current_path = None

    for line in text.splitlines():
        line_clean = line.strip()
        if line_clean.startswith("## "):
            # Any new H2 heading ends the previous section, path-like or not, so
            # bullets under a trailing prose heading can never be merged into a
            # file section.
            close_current()
            header_title = line_clean[3:].strip()
            if "/" in header_title or header_title.endswith((".c", ".h")):
                current_path = header_title
                current_section = {"kmod_path": current_path}
        elif current_section is not None and line_clean.startswith("- "):
            match = re.match(r"^-\s*([a-zA-Z0-9_]+)\s*:\s*(.*)$", line_clean)
            if match:
                key = match.group(1).strip()
                val = match.group(2).strip()
                if key in ("authors", "committers"):
                    current_section[key] = [a.strip() for a in val.split(",") if a.strip()]
                elif key == "kmod_path":
                    current_section[key] = val
                    current_path = val
                else:
                    current_section[key] = val

    close_current()
    return sections


def parse_inventory(path_or_text: str) -> List[Dict[str, Any]]:
    """Parse donor_inventory.yaml into a list of row dictionaries (validated)."""
    if yaml is None:
        raise RuntimeError("PyYAML is required to parse donor_inventory.yaml")

    if os.path.exists(path_or_text):
        with open(path_or_text, "r", encoding="utf-8") as f:
            data = yaml.safe_load(f)
    else:
        data = yaml.safe_load(path_or_text)

    if not isinstance(data, list):
        raise ProvenanceValidationError("donor_inventory.yaml must contain a list of rows")

    required = (
        "id",
        "matrix_row",
        "source_path",
        "source_commit",
        "source_symbol",
        "source_range",
        "disposition",
        "routine_owner",
        "required_fixture_ids",
        "adapter_id",
        "source_only_gap",
    )
    seen_ids: Set[str] = set()
    for row in data:
        if not isinstance(row, dict):
            raise ProvenanceValidationError("Each inventory entry must be a dictionary")
        rid = row.get("id", "<missing id>")
        for req in required:
            if req not in row:
                raise ProvenanceValidationError(f"Inventory row {rid} missing required field '{req}'")
        if row["id"] in seen_ids:
            raise ProvenanceValidationError(f"Inventory row id '{row['id']}' is not unique")
        seen_ids.add(row["id"])

        disp = normalize_disposition(row["disposition"])
        if disp not in VALID_DISPOSITIONS:
            raise ProvenanceValidationError(
                f"Inventory row {rid} has invalid disposition '{row['disposition']}'"
            )

        owner = str(row["routine_owner"]).strip()
        if not owner or not VALID_OWNER_RE.match(owner):
            raise ProvenanceValidationError(f"Inventory row {rid} has invalid routine_owner '{owner}'")

        adapter = str(row.get("adapter_id", "none")).strip().lower()
        if adapter not in VALID_ADAPTERS:
            raise ProvenanceValidationError(
                f"Inventory row {rid} has invalid adapter_id '{row.get('adapter_id')}'"
            )

        fixtures = row.get("required_fixture_ids") or []
        if not isinstance(fixtures, list) or any(not str(f).strip() for f in fixtures):
            raise ProvenanceValidationError(f"Inventory row {rid} has malformed required_fixture_ids")

        gap = row.get("source_only_gap")
        gap = str(gap).strip() if gap is not None else ""
        if disp == "donated_logic":
            if not fixtures:
                raise ProvenanceValidationError(
                    f"donated_logic row {rid} has no required fixture ids; an executable donation "
                    f"requires fixture ids and an adapter id, or reclassify the row "
                    f"(source_only_comparison / generated_schema_semantics) with an explicit gap"
                )
            if adapter == "none":
                raise ProvenanceValidationError(
                    f"donated_logic row {rid} has no executable adapter; an executable donation "
                    f"requires fixture ids and an adapter id, or reclassify the row "
                    f"(source_only_comparison / generated_schema_semantics) with an explicit gap"
                )
        else:
            if not gap:
                raise ProvenanceValidationError(
                    f"non-donated row {rid} (disposition {disp}) has no explicit source_only_gap rationale"
                )

        if is_kmod_path(str(row["source_path"])) and str(row["source_commit"]).strip() != KMOD_DONOR_COMMIT:
            raise ProvenanceValidationError(
                f"Inventory row {rid} cites kmod path '{row['source_path']}' but source_commit is "
                f"'{row['source_commit']}' (expected {KMOD_DONOR_COMMIT})"
            )

        parse_ranges(row["source_range"])  # validate shape

    return data


def check_partition(inventory: List[Dict[str, Any]]) -> None:
    """Refuse overlapping ranges with different dispositions or owners.

    An overlap is legal only as an explicit dependency record: the dependent row
    declares depends_on naming the owning row's id and is itself a
    source_only_comparison / generated_schema_semantics row (it records a
    dependency, not an ownership claim).
    """
    by_key: Dict[Tuple[str, str], List[Dict[str, Any]]] = defaultdict(list)
    for row in inventory:
        by_key[(str(row["source_path"]), str(row["source_commit"]))].append(row)

    for (path, _commit), rows in sorted(by_key.items()):
        parsed = [parse_ranges(r["source_range"]) for r in rows]
        for i in range(len(rows)):
            for j in range(i + 1, len(rows)):
                a, b = rows[i], rows[j]
                if (
                    normalize_disposition(a["disposition"]) == normalize_disposition(b["disposition"])
                    and str(a["routine_owner"]) == str(b["routine_owner"])
                ):
                    continue
                if not ranges_overlap(parsed[i], parsed[j]):
                    continue
                dep_a = a.get("depends_on") or []
                dep_b = b.get("depends_on") or []
                b_depends_on_a = (
                    b["id"] != a["id"]
                    and a["id"] in dep_b
                    and normalize_disposition(b["disposition"]) in ("source_only_comparison", "generated_schema_semantics")
                )
                a_depends_on_b = (
                    b["id"] in dep_a
                    and normalize_disposition(a["disposition"]) in ("source_only_comparison", "generated_schema_semantics")
                )
                if b_depends_on_a or a_depends_on_b:
                    continue
                raise ProvenanceValidationError(
                    f"inventory partition violation: {path} lines {a['source_range']} (row {a['id']}, "
                    f"{normalize_disposition(a['disposition'])}, owner {a['routine_owner']}) and "
                    f"{b['source_range']} (row {b['id']}, {normalize_disposition(b['disposition'])}, "
                    f"owner {b['routine_owner']}) overlap with different dispositions or owners; give the "
                    f"rows non-overlapping ranges or record one as a depends_on dependency of the other"
                )


def _git(repo: Optional[str], args: List[str]) -> subprocess.CompletedProcess:
    cmd = ["git"]
    if repo:
        cmd.extend(["-C", repo])
    cmd.extend(args)
    return subprocess.run(cmd, capture_output=True, text=True, check=False)


# Identities the ledger records by name alone. The corporate identity commits
# from an individual's mailbox, which a public ledger has no reason to carry.
# Dropping the mailbox is sound only because live verification holds two things:
# the history read is the one pinned by KMOD_DONOR_COMMIT_SHA, and each of these
# names commits from a single mailbox in it (_kmod_require_single_mailbox).
NAME_ONLY_IDENTITIES: Set[str] = {"Petabit Scale"}


def _ledger_identity(raw: str) -> str:
    """Map a `Name <address>` identity from git to the form the ledger records."""
    name = raw.split(" <", 1)[0]
    return name if name in NAME_ONLY_IDENTITIES else raw


def _observed_identities(stdout: str) -> List[str]:
    seen: List[str] = []
    for line in stdout.splitlines():
        identity = _ledger_identity(line.strip()) if line.strip() else ""
        if identity and identity not in seen:
            seen.append(identity)
    return seen


def _kmod_observed_authors(kmod_dir: str, path: str) -> List[str]:
    proc = _git(kmod_dir, ["log", "--follow", "--format=%an <%ae>", KMOD_DONOR_COMMIT, "--", path])
    if proc.returncode != 0:
        raise ProvenanceValidationError(
            f"--kmod-dir verification failed for {path}: git log error: {proc.stderr.strip()}"
        )
    return _observed_identities(proc.stdout)


def _kmod_observed_committers(kmod_dir: str, path: str) -> List[str]:
    proc = _git(kmod_dir, ["log", "--follow", "--format=%cn <%ce>", KMOD_DONOR_COMMIT, "--", path])
    if proc.returncode != 0:
        raise ProvenanceValidationError(
            f"--kmod-dir verification failed for {path}: git log error: {proc.stderr.strip()}"
        )
    return _observed_identities(proc.stdout)


def _kmod_require_single_mailbox(kmod_dir: str) -> None:
    """Refuse a pinned history in which a name-only identity has two mailboxes.

    The ledger attributes a name-only identity without recording its mailbox, so a
    second mailbox committing under the same display name would be attributed
    without ever appearing in the ledger.
    """
    proc = _git(kmod_dir, ["log", "--format=%an <%ae>%n%cn <%ce>", KMOD_DONOR_COMMIT])
    if proc.returncode != 0:
        raise ProvenanceValidationError(
            f"--kmod-dir verification failed: git log error: {proc.stderr.strip()}"
        )
    mailboxes: Dict[str, Set[str]] = defaultdict(set)
    for line in proc.stdout.splitlines():
        name, sep, mailbox = line.strip().partition(" <")
        if sep and name in NAME_ONLY_IDENTITIES:
            mailboxes[name].add(mailbox)
    for name, seen in sorted(mailboxes.items()):
        if len(seen) > 1:
            raise ProvenanceValidationError(
                f"--kmod-dir verification failed: name-only identity '{name}' commits from "
                f"{len(seen)} different mailboxes in the history at {KMOD_DONOR_COMMIT}; the "
                f"ledger records it by name alone, so each mailbox must be attributed separately"
            )


def _kmod_resolve_commit(kmod_dir: str, rev: str, what: str) -> str:
    """Resolve `rev` to a commit in the clone; an unresolvable revision is an error."""
    proc = _git(kmod_dir, ["rev-parse", "--verify", "--quiet", f"{rev}^{{commit}}"])
    sha = proc.stdout.strip()
    if proc.returncode != 0 or not sha:
        raise ProvenanceValidationError(
            f"--kmod-dir verification failed: {what} '{rev}' does not resolve to a commit in the "
            f"kmod clone (shallow, partial or wrong clone?); live verification cannot proceed"
        )
    return sha


def _kmod_exists_at(kmod_dir: str, rev: str, path: str) -> bool:
    # Resolve first: git exits 128 both for an absent path and for an unresolvable
    # revision, so only after the revision resolves does 128 mean "path absent".
    sha = _kmod_resolve_commit(kmod_dir, rev, f"revision for {path}")
    proc = _git(kmod_dir, ["cat-file", "-e", f"{sha}:{path}"])
    if proc.returncode not in (0, 128):
        raise ProvenanceValidationError(
            f"--kmod-dir verification failed for {path}: git cat-file error: {proc.stderr.strip()}"
        )
    return proc.returncode == 0


def validate_provenance(
    sections: Dict[str, Dict[str, Any]],
    inventory: List[Dict[str, Any]],
    require_landings: bool = False,
    git_repo: Optional[str] = None,
    kmod_dir: Optional[str] = None,
) -> Dict[str, Any]:
    """Validate ledger sections against donor inventory and authorship rules.

    Checks:
    - Omitted donor: every reused kmod path (donated_logic, source_only_comparison,
      generated_schema_semantics) must have a section (normalized disposition).
    - Changed source identity: every section maps to an inventory row and its
      donor_commit matches the inventory's source_commit.
    - Required section fields, including kmod_commits, pre_split_history, grant
      and lib_commits.
    - Wrong section author: authors equal the observed history in both directions.
    - Committer attribution: every listed committer is an attributed identity
      (including the GitHub web PR merge committer), committer lists equal the
      observed committer history in both directions (retained table and live
      --kmod-dir), and a `clear` verdict with any unattributed author or
      committer is refused.
    - Unattributed-plus-clear and unreachable/not-checked-plus-clear are refused.
    - Stopped files never record a landing.
    - Landings (if require_landings): lib_path present at the landing endpoint.
    - --kmod-dir (if given): the donor commit and split root must resolve in the
      clone; authors, committers and split-root existence are re-derived live; and
      every kmod_commits endpoint must resolve and contain the file.
    """
    if not ATTRIBUTED_IDENTITIES or not RETAINED_OBSERVED_AUTHORS or not KMOD_DONOR_COMMIT:
        raise ProvenanceValidationError("--authorship: load retained facts before validation")
    check_partition(inventory)

    if kmod_dir is not None:
        # Fail closed before any per-file check: both pins must resolve in the clone.
        donor = _kmod_resolve_commit(kmod_dir, KMOD_DONOR_COMMIT, "donor commit")
        if donor != KMOD_DONOR_COMMIT_SHA:
            raise ProvenanceValidationError(
                f"--kmod-dir verification failed: donor commit '{KMOD_DONOR_COMMIT}' resolves to "
                f"{donor} in the kmod clone, not the pinned {KMOD_DONOR_COMMIT_SHA} (wrong clone?)"
            )
        _kmod_resolve_commit(kmod_dir, KMOD_SPLIT_ROOT, "split root")
        _kmod_require_single_mailbox(kmod_dir)

    inventory_by_path: Dict[str, List[Dict[str, Any]]] = defaultdict(list)
    for row in inventory:
        inventory_by_path[str(row["source_path"])].append(row)

    # 1. Closed world: every reused kmod path must have a section.
    for source_path, rows in sorted(inventory_by_path.items()):
        reused = any(
            normalize_disposition(r["disposition"]) in SECTION_REQUIRED_DISPOSITIONS for r in rows
        )
        if reused and source_path not in sections:
            raise ProvenanceValidationError(
                f"omitted donor: {source_path} is reused (donated_logic, source_only_comparison or "
                f"generated_schema_semantics) in the inventory but has no ledger section"
            )

    cleared_files: List[str] = []
    stopped_files: List[str] = []

    for kmod_path, sec in sorted(sections.items()):
        if kmod_path not in inventory_by_path:
            raise ProvenanceValidationError(
                f"changed source identity: {kmod_path} in ledger does not match any inventory row"
            )

        inv_rows = inventory_by_path[kmod_path]

        # Required fields (fail closed: a missing or empty line is an error, never a pass).
        for field in REQUIRED_SECTION_FIELDS:
            value = sec.get(field)
            if field in ("authors", "committers"):
                if not value:
                    raise ProvenanceValidationError(
                        f"missing required field '{field}' in section {kmod_path}"
                    )
            elif field not in sec or not str(value).strip():
                raise ProvenanceValidationError(
                    f"missing required field '{field}' in section {kmod_path}"
                )

        sec_commit = str(sec.get("donor_commit", "")).strip().lower()
        if not sec_commit:
            raise ProvenanceValidationError(f"missing donor_commit in section {kmod_path}")
        commit_matched = False
        for r in inv_rows:
            inv_commit = str(r["source_commit"]).strip().lower()
            if inv_commit == "current" or sec_commit.startswith(inv_commit) or inv_commit.startswith(sec_commit):
                commit_matched = True
                break
        if not commit_matched:
            raise ProvenanceValidationError(
                f"changed source identity: {kmod_path} donor_commit '{sec_commit}' does not match "
                f"inventory commit '{inv_rows[0]['source_commit']}'"
            )

        authors: List[str] = sec.get("authors", [])
        verdict = str(sec.get("verdict", "")).strip().lower()
        if verdict not in ("clear", "stop"):
            raise ProvenanceValidationError(
                f"section {kmod_path} has invalid verdict '{sec.get('verdict')}'"
            )

        # Author attribution (every listed identity).
        for a in authors:
            if a not in ATTRIBUTED_IDENTITIES:
                if verdict == "clear":
                    raise ProvenanceValidationError(
                        f"unattributed-plus-clear: {kmod_path} has unattributed author '{a}' with verdict clear"
                    )
                raise ProvenanceValidationError(
                    f"wrong section author: '{a}' not recognized in attribution policy for {kmod_path}"
                )

        # Observed-history cross-check: closed world, both directions.
        if kmod_path not in RETAINED_OBSERVED_AUTHORS:
            raise ProvenanceValidationError(
                f"no retained observed-author evidence for {kmod_path}; the retained history table is "
                f"closed-world — re-derive authors from a the donor pin clone and record them"
            )
        observed = RETAINED_OBSERVED_AUTHORS[kmod_path]
        listed = set(authors)
        for a in sorted(listed - observed):
            raise ProvenanceValidationError(
                f"wrong section author: '{a}' is not in the observed history of {kmod_path}"
            )
        for a in sorted(observed - listed):
            raise ProvenanceValidationError(
                f"omitted section author: '{a}' is in the observed history of {kmod_path} "
                f"but is not attributed in its section (author lists must equal observed history, "
                f"automation identities included)"
            )

        # Committer attribution (every listed identity; the GitHub web PR merge
        # committer is attributed in the policy as platform automation that never
        # authors, and records merges performed by the PR author, not authorship).
        committers: List[str] = sec.get("committers", [])
        for c in committers:
            if c not in ATTRIBUTED_IDENTITIES:
                if verdict == "clear":
                    raise ProvenanceValidationError(
                        f"unattributed-plus-clear: {kmod_path} has unattributed committer '{c}' "
                        f"with verdict clear"
                    )
                raise ProvenanceValidationError(
                    f"wrong section committer: '{c}' not recognized in attribution policy "
                    f"for {kmod_path}"
                )

        # Observed-committer cross-check: closed world, both directions.
        if kmod_path not in RETAINED_OBSERVED_COMMITTERS:
            raise ProvenanceValidationError(
                f"no retained observed-committer evidence for {kmod_path}; the retained history "
                f"table is closed-world — re-derive committers from a the donor pin clone and record them"
            )
        observed_committers = RETAINED_OBSERVED_COMMITTERS[kmod_path]
        listed_committers = set(committers)
        for c in sorted(listed_committers - observed_committers):
            raise ProvenanceValidationError(
                f"wrong section committer: '{c}' is not in the observed committer history of {kmod_path}"
            )
        for c in sorted(observed_committers - listed_committers):
            raise ProvenanceValidationError(
                f"omitted section committer: '{c}' is in the observed committer history of "
                f"{kmod_path} but is not recorded in its section (committer lists must equal "
                f"observed history, automation identities included)"
            )

        # Optional live re-derivation from a kmod clone.
        if kmod_dir is not None:
            live = set(_kmod_observed_authors(kmod_dir, kmod_path))
            if live != set(authors):
                raise ProvenanceValidationError(
                    f"--kmod-dir author mismatch for {kmod_path}: section lists {sorted(set(authors))} "
                    f"but git log --follow at {KMOD_DONOR_COMMIT} shows {sorted(live)}"
                )
            live_committers = set(_kmod_observed_committers(kmod_dir, kmod_path))
            if live_committers != set(committers):
                raise ProvenanceValidationError(
                    f"--kmod-dir committer mismatch for {kmod_path}: section lists "
                    f"{sorted(set(committers))} but git log --follow at {KMOD_DONOR_COMMIT} "
                    f"shows {sorted(live_committers)}"
                )
            if _kmod_exists_at(kmod_dir, KMOD_SPLIT_ROOT, kmod_path):
                if verdict == "clear":
                    raise ProvenanceValidationError(
                        f"unreachable-plus-clear: {kmod_path} exists at split root {KMOD_SPLIT_ROOT} "
                        f"(pre-split history unreachable) but its verdict is clear"
                    )
            for endpoint in str(sec.get("kmod_commits", "")).split(".."):
                endpoint = endpoint.strip()
                if not endpoint:
                    continue
                _kmod_resolve_commit(kmod_dir, endpoint, f"{kmod_path} kmod_commits endpoint")
                if not _kmod_exists_at(kmod_dir, endpoint, kmod_path):
                    raise ProvenanceValidationError(
                        f"--kmod-dir commit check failed: {kmod_path} is not present at its "
                        f"kmod_commits endpoint '{endpoint}' in the kmod clone"
                    )

        # Pre-split history: `clear` requires affirmative evidence of a full check.
        pre_split = str(sec.get("pre_split_history", "")).strip().lower()
        if verdict == "clear":
            if "unreachable" in pre_split:
                raise ProvenanceValidationError(
                    f"unreachable-plus-clear: {kmod_path} has unreachable pre-split history but verdict is clear"
                )
            if not pre_split.startswith(PRE_SPLIT_CHECKED_PREFIX):
                raise ProvenanceValidationError(
                    f"pre-split history not affirmatively checked: {kmod_path} has verdict clear but "
                    f"pre_split_history is "
                    f"'{sec.get('pre_split_history') or '<absent>'}' — clear requires positive evidence "
                    f"that full history was checked (a 'checked:' pre_split_history value); absent, "
                    f"'not checked', 'unknown' or unreachable values require verdict stop"
                )
        else:
            if not pre_split:
                raise ProvenanceValidationError(
                    f"section {kmod_path} carries verdict stop without a pre_split_history reason"
                )

        lib_commits = str(sec.get("lib_commits", "")).strip()
        if verdict == "stop":
            stopped_files.append(kmod_path)
            if lib_commits and lib_commits.lower() != "pending":
                raise ProvenanceValidationError(
                    f"stopped file cannot have landed: {kmod_path} has verdict stop but lib_commits "
                    f"is '{lib_commits}'"
                )
        else:
            cleared_files.append(kmod_path)
            if require_landings:
                if not lib_commits or lib_commits.lower() == "pending":
                    raise ProvenanceValidationError(
                        f"pending landing: {kmod_path} has pending landing commits '{lib_commits}'"
                    )
                lib_path = str(sec.get("lib_path", "")).strip()
                if not lib_path:
                    raise ProvenanceValidationError(
                        f"pending landing: {kmod_path} has no lib_path declared for landed port"
                    )
                landing_endpoint = lib_commits.split("..")[-1].strip()
                proc = _git(git_repo, ["cat-file", "-e", f"{landing_endpoint}:{lib_path}"])
                if proc.returncode != 0:
                    raise ProvenanceValidationError(
                        f"path present only at another commit: {lib_path} is not present at commit "
                        f"{landing_endpoint} (the landing endpoint of {kmod_path})"
                    )

    return {
        "total_sections": len(sections),
        "cleared_count": len(cleared_files),
        "stopped_count": len(stopped_files),
        "cleared_files": cleared_files,
        "stopped_files": stopped_files,
    }


def default_lib_repo() -> str:
    """The openxcvr-lib repository root that owns this tool (never the cwd)."""
    return str(Path(__file__).resolve().parent.parent)


def main(argv: Optional[List[str]] = None) -> int:
    parser = argparse.ArgumentParser(description="Validate OpenXCVR provenance ledger and donor inventory")
    parser.add_argument("--provenance", default=str(Path(default_lib_repo()) / (Path(__file__).stem.removeprefix("check-").upper() + ".md")), help="Path to the reuse ledger")
    parser.add_argument(
        "--inventory",
        default=str(Path(default_lib_repo()) / "tests/fixtures/parity/donor_inventory.yaml"),
        help="Path to donor_inventory.yaml",
    )
    parser.add_argument(
        "--require-landings",
        action="store_true",
        help="Enforce that all ports have landed with valid lib_path and lib_commits",
    )
    parser.add_argument(
        "--lib-repo",
        default=None,
        help="openxcvr-lib repository for --require-landings path checks "
        "(default: the repository containing this tool, never the cwd)",
    )
    parser.add_argument(
        "--kmod-dir",
        default=None,
        help="Path to an donor repository clone at the donor pin for live author/committer/split-root verification",
    )
    parser.add_argument("--authorship", type=Path, default=Path(default_lib_repo()) / "tests/fixtures/parity/authorship.json",
                        help="Retained identity grants and observed history (JSON)")
    args = parser.parse_args(argv)

    if not os.path.exists(args.provenance):
        print(f"ERROR: --provenance file '{args.provenance}' not found", file=sys.stderr)
        return 1
    if not os.path.exists(args.inventory):
        print(f"ERROR: --inventory file '{args.inventory}' not found", file=sys.stderr)
        return 1
    if args.kmod_dir is not None and not os.path.isdir(args.kmod_dir):
        print(f"ERROR: --kmod-dir '{args.kmod_dir}' is not a directory", file=sys.stderr)
        return 1

    git_repo = args.lib_repo or default_lib_repo()

    try:
        load_authorship(args.authorship)
        sections = parse_provenance(args.provenance)
        inventory = parse_inventory(args.inventory)
        result = validate_provenance(
            sections=sections,
            inventory=inventory,
            require_landings=args.require_landings,
            git_repo=git_repo,
            kmod_dir=args.kmod_dir,
        )
    except ProvenanceValidationError as e:
        print(f"LEDGER VALIDATION FAILED: {e}", file=sys.stderr)
        return 1
    except Exception as e:
        print(f"ERROR: {e}", file=sys.stderr)
        return 1

    print("OpenXCVR Provenance Validation: OK")
    print(f"  Total sections in ledger: {result['total_sections']}")
    print(f"  Cleared files (verdict clear)  : {result['cleared_count']}")
    print(f"  Stopped files (verdict stop)   : {result['stopped_count']}")
    if result["stopped_files"]:
        stopped = ", ".join(result["stopped_files"][:5])
        more = "" if len(result["stopped_files"]) <= 5 else f" (+{len(result['stopped_files'])-5} more)"
        print(f"  Stopped paths (pre-split unreachable): {stopped}{more}")

    return 0


if __name__ == "__main__":
    sys.exit(main())
